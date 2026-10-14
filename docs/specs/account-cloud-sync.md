# Accounts and encrypted synchronization

## Scope

Go service with PostgreSQL, native desktop encryption, Google OpenID Connect and email one-time codes. Account profile contains a display name and normalized PNG avatar. Sidebar shows login or current user. Preferences contain auto sync, manual sync, device authorization and recovery.

First association requires an explicit choice: merge/download, upload only, or no synchronization. Each local profile receives an independent opaque cloud UUID; legacy profile IDs never identify cloud data. Cloud deletions use persistent tombstones. Local detach records a subscription exclusion, not a cloud deletion. Switching accounts isolates credentials, encryption keys, queues and baselines.

## Security contract

Account authentication does not unlock encrypted data. First device creates a random account root and recovery secret. Native OS keyring holds credentials/private keys. Existing authorized device approval requires comparing a fingerprint; alternatively the recovery secret unlocks the recovery envelope. The server never receives root or recovery secrets. Lost devices are revoked; previously downloaded plaintext cannot be remotely recalled.

Snapshots use XChaCha20-Poly1305 with random per-object data keys wrapped by the account root. Associated data binds format, account UUID, object UUID, type, revision and key epoch. Device envelopes use RFC 9180 HPKE. Cloud records are opaque ciphertext and minimal version metadata. No browser cookie values, proxy passwords, account keys or one-time codes in logs.

Device authorization records additionally carry an account-root HMAC over account/device/public-key/epoch. Device approval requires comparison in both directions: the existing device verifies the enrollment fingerprint; the new device verifies the encryption-space code from an already trusted device. New roots are staged separately in the OS keyring until this verification or a valid recovery secret confirms them. Confirmed roots cannot be replaced by a cloud response at the same epoch. Approved-device revocation atomically rotates all latest objects and remaining device envelopes.

Clients refuse lower object revisions/key epochs and changed content at a confirmed revision. These checkpoints survive local profile recreation. This detects replay against a device's known history; a new device cannot independently prove history it has never observed. Encryption does not prevent an unavailable server from withholding data, and a compromised authorized endpoint can read its own decrypted data.

HTTPS required in production. API authorization checks account and approved device for every ciphertext operation. Database transactions enforce compare-and-swap revisions and session leases. Short-lived access tokens, hashed opaque refresh tokens, rotation, revocation, rate limits, bounded bodies and fail-closed provider configuration.

## Synchronization behavior

Configuration uses a three-way baseline merge. Independent fields merge; proxy credentials/settings are one atomic group. Concurrent edits, deletion versus modification and tombstone restoration require user decisions. Revision counters, never wall-clock timestamps, determine concurrency. Configuration excludes executable paths, process state, relay ports, installed fonts, system timezone, terminal paths, diagnostic results and local UI preferences.

Website session data is an atomic browser-neutral snapshot of cookies, local storage and IndexedDB for managed profiles. Browser API export/import preserves browser encryption on each device. It excludes saved passwords, payment data, passkeys and unrelated normal browser profiles. Import only while managed browser is closed; validate origins, values and size. Unsupported storage must produce an explicit error, never silently declare success. Session snapshots have a device lease and fenced generation, preventing a previous device from overwriting a new owner. Cloud restoration cannot guarantee acceptance by third-party websites.

## Validation

Service integration tests run against real PostgreSQL. Verify cross-account isolation, pending-device restrictions, OTP attempts/expiry/replay, rotating refresh replay, CAS conflicts, deletion conflicts, session lease takeover fencing, revocation and data limits. Native tests verify AEAD tampering/AAD/epoch failures, recovery and device envelope isolation, merge decisions and safe browser-state validation. Desktop tests verify login, account controls, sync choices and conflict UI. No production authentication bypass or test-only OTP endpoint.

## Browser compatibility limits

Verified data includes persistent Secure/HttpOnly cookies, LocalStorage and IndexedDB, with browser API readback and restart. Chrome/Edge profile files alone are insufficient for cross-device cookie migration because browser encryption is device-bound; the exporter/importer uses the browser APIs on each endpoint. Website servers may revoke sessions or require another sign-in after a device change. Cookies that expire at browser exit may not survive the close/export cycle.

Partitioned persistent Cookies, LocalStorage and IndexedDB retain their top-level-site identities in snapshot schema 2. Schema 1 remains readable; older clients reject schema 2. Native Chromium storage keys and Firefox origin attributes distinguish ordinary third-party partitions and same-site frames behind cross-site ancestors. See `partitioned-session-storage-validation.md` for current verification. Opaque/nonce partitions, unsupported Firefox containers, named storage buckets and non-exportable cryptographic device keys fail explicitly rather than being silently dropped. Structured-clone data supports cycles, binary views, Blob/File, Map/Set, dates, BigInt and extractable CryptoKey values. IndexedDB schemas/indexes/records are preserved; the past auto-increment generator position after deleted keys is not exportable through standard browser APIs. Saved passwords, payment data, passkeys, CacheStorage, service-worker registrations and sessionStorage are outside the snapshot scope.

The current native implementation was tested on installed Windows Chrome, Edge and Firefox. Earlier synthetic API probes verified basic persistent cookie/LocalStorage/IndexedDB transfer in both directions between Windows and macOS. A macOS build and full cloud-service/native-client integration of this implementation still require validation on the Mac; prior probe results do not replace that check.
