//! All secret material stays in the native process. The server receives only envelopes.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use ed25519_dalek::{Signer, SigningKey};
use hkdf::Hkdf;
use hpke::{
    aead::ChaCha20Poly1305, kdf::HkdfSha256, kem::X25519HkdfSha256, Deserializable,
    Kem as KemTrait, OpModeR, OpModeS, Serializable,
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

pub type Key = Zeroizing<[u8; 32]>;
pub fn device_proof(
    root: &[u8; 32],
    account: &str,
    device: &str,
    public: &str,
    epoch: u32,
) -> String {
    use hmac::Mac;
    let mut mac = <hmac::Hmac<Sha256> as Mac>::new_from_slice(root).expect("fixed key");
    mac.update(
        format!("nodecloak-device-approval-v1:{account}:{device}:{epoch}:{public}").as_bytes(),
    );
    B64.encode(mac.finalize().into_bytes())
}
pub fn verify_device_proof(
    root: &[u8; 32],
    account: &str,
    device: &str,
    public: &str,
    epoch: u32,
    proof: &str,
) -> bool {
    use hmac::Mac;
    let Ok(bytes) = B64.decode(proof) else {
        return false;
    };
    let mut mac = <hmac::Hmac<Sha256> as Mac>::new_from_slice(root).expect("fixed key");
    mac.update(
        format!("nodecloak-device-approval-v1:{account}:{device}:{epoch}:{public}").as_bytes(),
    );
    mac.verify_slice(&bytes).is_ok()
}
pub fn vault_code(account: &str, root: &[u8; 32]) -> String {
    use sha2::Digest;
    let mut h = Sha256::new();
    h.update(b"nodecloak-vault-code-v1:");
    h.update(account.as_bytes());
    h.update(root);
    let digest = format!("{:x}", h.finalize());
    digest[digest.len() - 12..].into()
}
#[cfg(test)]
mod roster_tests {
    use super::*;
    #[test]
    fn cloud_cannot_substitute_approved_public_keys_or_encryption_epoch() {
        let root = random_key().unwrap();
        let proof = device_proof(&root, "account", "device", "public", 1);
        assert!(verify_device_proof(
            &root, "account", "device", "public", 1, &proof
        ));
        for (account, device, public, epoch) in [
            ("other", "device", "public", 1),
            ("account", "other", "public", 1),
            ("account", "device", "attacker", 1),
            ("account", "device", "public", 2),
        ] {
            assert!(!verify_device_proof(
                &root, account, device, public, epoch, &proof
            ));
        }
        assert!(!verify_device_proof(
            &random_key().unwrap(),
            "account",
            "device",
            "public",
            1,
            &proof
        ));
        assert_ne!(vault_code("account", &root), vault_code("other", &root));
    }
}
use super::session_limits::{FRAME_HEADER_BYTES, LEGACY_BYTES, SNAPSHOT_BYTES, TOO_LARGE};
const ERROR: &str = "加密数据校验失败，身份、版本或密钥不匹配";
#[derive(Clone, Copy, Serialize)]
pub struct Context<'a> {
    pub account: &'a str,
    pub object: &'a str,
    pub kind: &'a str,
    pub revision: u64,
    pub epoch: u32,
    pub deleted: bool,
    pub generation: u64,
    pub writer: &'a str,
}
impl Context<'_> {
    fn aad(&self) -> Result<Vec<u8>, String> {
        if self.epoch == 0
            || self.revision == 0
            || self.account.is_empty()
            || self.object.is_empty()
        {
            return Err(ERROR.into());
        };
        serde_json::to_vec(&("nodecloak-snapshot-v1", self)).map_err(|_| ERROR.into())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Envelope {
    pub version: u8,
    pub nonce: String,
    pub ciphertext: String,
    pub key_nonce: String,
    pub key_ciphertext: String,
}
pub fn random_key() -> Result<Key, String> {
    let mut k = Zeroizing::new([0; 32]);
    getrandom::fill(k.as_mut()).map_err(|_| "系统随机数不可用")?;
    Ok(k)
}
fn nonce() -> Result<[u8; 24], String> {
    let mut n = [0; 24];
    getrandom::fill(&mut n).map_err(|_| "系统随机数不可用")?;
    Ok(n)
}
fn encrypt(key: &[u8; 32], n: &[u8; 24], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, String> {
    XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(n), Payload { msg: plain, aad })
        .map_err(|_| ERROR.into())
}
fn decrypt(
    key: &[u8; 32],
    n: &[u8; 24],
    aad: &[u8],
    cipher: &[u8],
) -> Result<Zeroizing<Vec<u8>>, String> {
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(n), Payload { msg: cipher, aad })
        .map(Zeroizing::new)
        .map_err(|_| ERROR.into())
}
fn decode<const N: usize>(s: &str) -> Result<[u8; N], String> {
    if s.len() > N * 2 + 8 {
        return Err(ERROR.into());
    };
    B64.decode(s)
        .map_err(|_| ERROR.to_string())?
        .try_into()
        .map_err(|_| ERROR.into())
}
pub fn seal(root: &[u8; 32], ctx: &Context, plain: &[u8]) -> Result<Envelope, String> {
    if plain.len() > SNAPSHOT_BYTES {
        return Err(TOO_LARGE.into());
    };
    let version=if plain.len()>LEGACY_BYTES {2} else {1};
    let frame=if version==2 {Some(super::session_payload::encode(plain)?)} else {None};
    let data=frame.as_ref().map(|v|v.as_slice()).unwrap_or(plain);
    let aad = envelope_aad(ctx,version)?;
    let key = random_key()?;
    let n = nonce()?;
    let kn = nonce()?;
    let mut wrap_aad = b"nodecloak-data-key-v1:".to_vec();
    wrap_aad.extend(&aad);
    Ok(Envelope {
        version,
        nonce: B64.encode(n),
        ciphertext: B64.encode(encrypt(&key, &n, &aad, data)?),
        key_nonce: B64.encode(kn),
        key_ciphertext: B64.encode(encrypt(root, &kn, &wrap_aad, key.as_slice())?),
    })
}
fn envelope_aad(ctx:&Context,version:u8)->Result<Vec<u8>,String>{
    let mut aad=ctx.aad()?;
    match version {
        1=>{},
        2=>aad.extend_from_slice(b":nodecloak-large-snapshot-v2"),
        _=>return Err("此登录快照需要更新 NodeCloak 后才能读取，本机数据已保留".into()),
    }
    Ok(aad)
}
pub fn open(root: &[u8; 32], ctx: &Context, e: &Envelope) -> Result<Zeroizing<Vec<u8>>, String> {
    let aad = envelope_aad(ctx,e.version)?;
    let max=if e.version==1 {LEGACY_BYTES} else {SNAPSHOT_BYTES+FRAME_HEADER_BYTES};
    if e.ciphertext.len() > (max + 16 + 2) / 3 * 4
        || e.key_ciphertext.len() > 80
    {
        return Err(ERROR.into());
    };
    let mut wa = b"nodecloak-data-key-v1:".to_vec();
    wa.extend(&aad);
    let key_bytes = decrypt(
        root,
        &decode(&e.key_nonce)?,
        &wa,
        &B64.decode(&e.key_ciphertext).map_err(|_| ERROR)?,
    )?;
    let key: Key = Zeroizing::new(key_bytes.as_slice().try_into().map_err(|_| ERROR)?);
    let data=decrypt(
        &key,
        &decode(&e.nonce)?,
        &aad,
        &B64.decode(&e.ciphertext).map_err(|_| ERROR)?,
    )?;
    if e.version==2 {super::session_payload::decode(&data)} else {Ok(data)}
}
pub struct DeviceKeys {
    pub private_key: Key,
    pub public_key: [u8; 32],
}
type Kem = X25519HkdfSha256;
impl DeviceKeys {
    pub fn generate() -> Result<Self, String> {
        let seed = random_key()?;
        let mut rng = ChaCha20Rng::from_seed(*seed);
        let (sk, pk) = Kem::gen_keypair(&mut rng);
        Ok(Self {
            private_key: Zeroizing::new(sk.to_bytes().as_slice().try_into().map_err(|_| ERROR)?),
            public_key: pk.to_bytes().as_slice().try_into().map_err(|_| ERROR)?,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceEnvelope {
    version: u8,
    enc: String,
    ciphertext: String,
}
fn device_info(account: &str, device: &str, epoch: u32) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&("nodecloak-device-hpke-v1", account, device, epoch))
        .map_err(|_| ERROR.into())
}
pub fn device_seal(
    public: &[u8; 32],
    account: &str,
    device: &str,
    epoch: u32,
    root: &[u8; 32],
) -> Result<String, String> {
    let pk = <Kem as KemTrait>::PublicKey::from_bytes(public).map_err(|_| ERROR)?;
    let info = device_info(account, device, epoch)?;
    let seed = random_key()?;
    let mut rng = ChaCha20Rng::from_seed(*seed);
    let (enc, cipher) = hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, Kem, _>(
        &OpModeS::Base,
        &pk,
        &info,
        root,
        &info,
        &mut rng,
    )
    .map_err(|_| ERROR)?;
    serde_json::to_string(&DeviceEnvelope {
        version: 1,
        enc: B64.encode(enc.to_bytes()),
        ciphertext: B64.encode(cipher),
    })
    .map_err(|_| ERROR.into())
}
pub fn device_open(
    private: &[u8; 32],
    account: &str,
    device: &str,
    epoch: u32,
    env: &str,
) -> Result<Key, String> {
    if env.len() > 1024 {
        return Err(ERROR.into());
    };
    let e: DeviceEnvelope = serde_json::from_str(env).map_err(|_| ERROR)?;
    if e.version != 1 {
        return Err(ERROR.into());
    };
    let sk = <Kem as KemTrait>::PrivateKey::from_bytes(private).map_err(|_| ERROR)?;
    let enc =
        <Kem as KemTrait>::EncappedKey::from_bytes(&decode::<32>(&e.enc)?).map_err(|_| ERROR)?;
    let info = device_info(account, device, epoch)?;
    let plain = Zeroizing::new(
        hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, Kem>(
            &OpModeR::Base,
            &sk,
            &enc,
            &info,
            &B64.decode(e.ciphertext).map_err(|_| ERROR)?,
            &info,
        )
        .map_err(|_| ERROR)?,
    );
    Ok(Zeroizing::new(
        plain.as_slice().try_into().map_err(|_| ERROR)?,
    ))
}
fn derive(secret: &[u8; 32], purpose: &[u8]) -> Result<Key, String> {
    let mut k = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(b"nodecloak-recovery-v1"), secret)
        .expand(purpose, k.as_mut())
        .map_err(|_| ERROR)?;
    Ok(k)
}
pub fn recovery_seal(
    secret: &[u8; 32],
    account: &str,
    epoch: u32,
    root: &[u8; 32],
) -> Result<String, String> {
    let key = derive(secret, b"root-wrap")?;
    serde_json::to_string(&seal(
        &key,
        &Context {
            account,
            object: "vault",
            kind: "recovery",
            revision: 1,
            epoch,
            deleted: false,
            generation: 0,
            writer: "recovery",
        },
        root,
    )?)
    .map_err(|_| ERROR.into())
}
pub fn recovery_open(
    secret: &[u8; 32],
    account: &str,
    epoch: u32,
    env: &str,
) -> Result<Key, String> {
    if env.len() > 2048 {
        return Err(ERROR.into());
    };
    let key = derive(secret, b"root-wrap")?;
    let e = serde_json::from_str(env).map_err(|_| ERROR)?;
    let plain = open(
        &key,
        &Context {
            account,
            object: "vault",
            kind: "recovery",
            revision: 1,
            epoch,
            deleted: false,
            generation: 0,
            writer: "recovery",
        },
        &e,
    )?;
    Ok(Zeroizing::new(
        plain.as_slice().try_into().map_err(|_| ERROR)?,
    ))
}
pub fn recovery_verifier(secret: &[u8; 32]) -> Result<[u8; 32], String> {
    Ok(
        SigningKey::from_bytes(&*derive(secret, b"approval-signing")?)
            .verifying_key()
            .to_bytes(),
    )
}
pub fn recovery_sign(secret: &[u8; 32], message: &[u8]) -> Result<[u8; 64], String> {
    Ok(
        SigningKey::from_bytes(&*derive(secret, b"approval-signing")?)
            .sign(message)
            .to_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Context<'static> {
        Context {
            account: "a",
            object: "p",
            kind: "config",
            revision: 7,
            epoch: 1,
            deleted: false,
            generation: 0,
            writer: "device",
        }
    }
    #[test]
    fn ciphertext_requires_exact_identity_revision_and_epoch() {
        let root = random_key().unwrap();
        let data = b"session-cookie-secret";
        let envelope = seal(&root, &context(), data).unwrap();
        assert_eq!(open(&root, &context(), &envelope).unwrap().as_slice(), data);
        assert!(!serde_json::to_string(&envelope)
            .unwrap()
            .contains("session-cookie-secret"));
        for ctx in [
            Context {
                account: "other",
                ..context()
            },
            Context {
                object: "other",
                ..context()
            },
            Context {
                kind: "session",
                ..context()
            },
            Context {
                revision: 8,
                ..context()
            },
            Context {
                epoch: 2,
                ..context()
            },
            Context {
                deleted: true,
                ..context()
            },
            Context {
                writer: "other",
                ..context()
            },
            Context {
                generation: 1,
                ..context()
            },
        ] {
            assert!(open(&root, &ctx, &envelope).is_err());
        }
        assert!(open(&random_key().unwrap(), &context(), &envelope).is_err());
    }
    #[test]
    fn tampering_is_rejected_and_nonces_are_fresh() {
        let root = random_key().unwrap();
        let a = seal(&root, &context(), b"cookie").unwrap();
        let b = seal(&root, &context(), b"cookie").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.key_nonce, b.key_nonce);
        let mut bad = a.clone();
        bad.ciphertext = "AAAA".into();
        assert!(open(&root, &context(), &bad).is_err());
        bad = a.clone();
        bad.key_ciphertext = b.key_ciphertext;
        assert!(open(&root, &context(), &bad).is_err());
    }
    #[test]
    fn large_compressible_sessions_round_trip_with_a_versioned_envelope() {
        let root=random_key().unwrap();let plain=vec![b'x';9<<20];
        let ctx=Context{kind:"session",..context()};
        let envelope=seal(&root,&ctx,&plain).expect("a normal session larger than 8 MiB must sync");
        assert_eq!(envelope.version,2);assert!(envelope.ciphertext.len()<200000);
        assert_eq!(open(&root,&ctx,&envelope).unwrap().as_slice(),plain);
        let mut downgraded=envelope.clone();downgraded.version=1;
        assert!(open(&root,&ctx,&downgraded).is_err());
        assert!(open(&root,&Context{revision:8,..ctx},&envelope).is_err());
    }
    #[test]
    fn incompressible_large_sessions_are_not_silently_truncated() {
        let root=random_key().unwrap();let mut plain=vec![0;9<<20];getrandom::fill(&mut plain).unwrap();
        let envelope=seal(&root,&context(),&plain).expect("incompressible sessions must also sync");
        assert_eq!(envelope.version,2);
        assert_eq!(open(&root,&context(),&envelope).unwrap().as_slice(),plain);
    }
    #[test]
    fn small_writes_keep_legacy_format_and_oversize_input_is_explicit() {
        let root=random_key().unwrap();let small=seal(&root,&context(),b"legacy-login").unwrap();
        assert_eq!(small.version,1);assert_eq!(open(&root,&context(),&small).unwrap().as_slice(),b"legacy-login");
        assert!(seal(&root,&context(),&vec![0;(64<<20)+1]).err().unwrap().contains("64 MiB"));
    }
    #[test]
    fn device_envelope_only_opens_on_target_device_and_account() {
        let a = DeviceKeys::generate().unwrap();
        let b = DeviceKeys::generate().unwrap();
        let root = random_key().unwrap();
        let env = device_seal(&a.public_key, "account", "device", 1, &root).unwrap();
        assert_eq!(
            device_open(&a.private_key, "account", "device", 1, &env)
                .unwrap()
                .as_slice(),
            root.as_slice()
        );
        assert!(device_open(&b.private_key, "account", "device", 1, &env).is_err());
        assert!(device_open(&a.private_key, "other", "device", 1, &env).is_err());
        assert!(device_open(&a.private_key, "account", "other", 1, &env).is_err());
    }
    #[test]
    fn recovery_secret_unlocks_and_signs_without_exposing_root() {
        let root = random_key().unwrap();
        let secret = random_key().unwrap();
        let env = recovery_seal(&secret, "account", 1, &root).unwrap();
        assert_eq!(
            recovery_open(&secret, "account", 1, &env)
                .unwrap()
                .as_slice(),
            root.as_slice()
        );
        assert!(recovery_open(&random_key().unwrap(), "account", 1, &env).is_err());
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let public = recovery_verifier(&secret).unwrap();
        let key = VerifyingKey::from_bytes(&public).unwrap();
        let sig =
            Signature::from_slice(&recovery_sign(&secret, b"one-time-challenge").unwrap()).unwrap();
        assert!(key.verify(b"one-time-challenge", &sig).is_ok());
        assert!(key.verify(b"other-challenge", &sig).is_err());
    }
}
