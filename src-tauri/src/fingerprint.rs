//! Persistent, per-profile readback protection. This seed is not an auth key.
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub version: u8,
    pub seed: String,
    pub enabled: bool,
    pub canvas_noise: bool,
    pub font_noise: bool,
    pub webgl_noise: bool,
    pub audio_noise: bool,
    pub media_noise: bool,
}
pub fn seed() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "系统随机数不可用")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn fresh() -> Result<Settings, String> {
    Ok(Settings {
        version: 1,
        seed: seed()?,
        enabled: true,
        canvas_noise: true,
        font_noise: true,
        webgl_noise: true,
        audio_noise: true,
        media_noise: true,
    })
}
pub fn validate(s: &Settings) -> Result<(), String> {
    if s.version != 1
        || s.seed.len() != 64
        || !s
            .seed
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("指纹种子或版本无效".into());
    }
    Ok(())
}
pub fn script(s: &Settings) -> Result<String, String> {
    validate(s)?;
    Ok(format!(
        "({})({})",
        include_str!("fingerprint.js"),
        serde_json::to_string(s).map_err(|_| "指纹设置无效")?
    ))
}
pub fn preload(s: &Settings) -> Result<String, String> {
    Ok(format!("()=>{{{};}}", script(s)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_or_future_fingerprints_never_become_browser_scripts() {
        let valid = fresh().unwrap();
        validate(&valid).unwrap();
        for seed in [
            "",
            "abc",
            &"A".repeat(64),
            &"0".repeat(63),
            &"g".repeat(64),
            "\";alert(1)//",
        ] {
            let mut invalid = valid.clone();
            invalid.seed = seed.into();
            assert!(validate(&invalid).is_err());
            assert!(script(&invalid).is_err());
        }
        let mut future = valid.clone();
        future.version = 2;
        assert!(script(&future).is_err());
        let mut unknown = serde_json::to_value(valid).unwrap();
        unknown["unknownProtection"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Settings>(unknown).is_err());
    }
}
