# Partitioned persistent session storage

This change replaces the blanket rejection of normal partitioned storage with
partition-aware export and restore for Chrome, Edge and Firefox. It is a local
implementation; it has not been released or deployed.

## Data and isolation

Session snapshots retain an origin and optional top-level-site/ancestor identity
for each LocalStorage/IndexedDB namespace. Persistent Cookies retain partition
identity, SameSite, expiry, Secure and HttpOnly attributes. Firefox snapshots also
retain its native partition tuple and the distinction between CHIPS Cookies and
automatically partitioned Cookies. Snapshot schema 2 carries these fields;
schema 1 remains readable, while older clients reject schema 2.

Chromium discovery reads Local Storage LevelDB metadata and both legacy
`Default/QuotaManager` and current `Default/WebStorage/QuotaManager` bucket
indexes. The latter is necessary for an IndexedDB-only partition, whose directory
is a numeric bucket rather than an origin name. Extension buckets are outside the
web snapshot and ignored. Firefox discovery reads origin attributes in storage
directory names and Cookie records in the staged `cookies.sqlite` database.
Cookie schemas 13–17 are handled, including the seconds-to-milliseconds expiry
change at schema 16.

The isolated browser recreates the top-level page and frame ancestry using empty,
intercepted HTML, then invokes the existing structured-clone codec in the selected
realm. Chromium's actual storage key is checked before importing. Firefox native
keys are checked after the staged browser closes. Same-site documents behind a
cross-site ancestor retain their separate storage namespace. Browser policy still
determines whether unpartitioned Cookies are additionally visible in a frame.

Firefox BiDi does not return native attributes on every Cookie and accepts only
integer-second expiry on import. Partitioned Cookies are read/written from the
closed staged database; unpartitioned Cookie expiry is restored to its original
native precision there as well. Firefox then loads the records again. Cookie
verification checks value, scope, partition attributes and expiry
with less than one second of allowed browser API quantization. No user Cookie
values are logged by production code.

## Failure and synchronization behavior

Export works on a closed-profile copy. Restore changes only a staged copy, clears
removed supported namespaces independently, and verifies Cookie and website data
before replacing the original profile. Existing recovery journaling handles
interrupted replacement. Unsupported Firefox Cookie-only containers are rejected
before Cookie deletion. Browser processes are closed before reading native data;
Windows process matching accepts both extended and ordinary path spellings.

Cloud encryption, account/device authorization, session leases, revision fencing,
conflict handling and share authorization are unchanged. The new snapshot has an
encryption/decryption round-trip test through the existing session AEAD context.

Opaque/nonce partitions, unsupported Firefox container attributes, named storage
buckets and non-exportable cryptographic keys still fail safely rather than being
flattened or partially uploaded. A same-site Firefox partition without the native
foreign-ancestor marker is not recreated as ordinary first-party storage. The
owner's existing local-launch fallback remains for unsupported namespaces;
received-share authorization and configuration conflicts still apply.

Snapshot scope remains persistent Cookies, LocalStorage and IndexedDB. Website
servers may require reauthentication after migration. This does not include
sessionStorage, CacheStorage, service-worker registrations, saved passwords,
passkeys or a browser-exit Cookie that has already disappeared.

## Verification

All browser fixtures are synthetic, offline, unique temporary profiles. No real
account, website login or user profile is used. The installed-browser regression
imports distinct same-named Cookies, LocalStorage keys and IndexedDB databases
under two top-level sites, an unpartitioned origin, and a same-site frame behind a
cross-site ancestor. It includes an IndexedDB-only partition and a Default
SameSite Cookie. It then exports/restores into a second profile, restarts, checks
native `document.cookie` partition visibility, rejects a malformed structured
clone without altering the original, and removes one partition while preserving
the others.

The expanded regression passed on Windows Chrome 155.0.8059.40 (23.87 s), Edge
155.0.4283.45 (25.12 s) and Firefox 158.0 (154.39 s). A separate Firefox regression
first reproduced BiDi rejection of a fractional-second expiry, then passed
closed-profile import/export with the exact original fractional expiry after the
native precision fix (40.16 s).
The existing schema-1 Cookie/LocalStorage/IndexedDB regression also passed on all
three installed browsers after the final changes (95.67 s). Independent code
review found no remaining P1/P2 issues after the parser and expiry fixes.

- Rust library suite: 134 passed, 0 failed, 14 explicitly ignored integration/live tests.
- React/Vitest suite: 134 passed across 30 files.
- Structured-clone codec: 4 passed, including malformed graph and non-exportable key rejection.
- `cargo check --locked --lib`, `npm run build` and `git diff --check`: passed.
- Parser/database fixtures cover invalid native tuples, opaque/nonce keys,
  unsupported containers, both quota paths, and legacy/current Firefox expiry units.

These tests run on Windows; macOS/Linux native behavior and real website
reauthentication have not been validated by this change.

Primary implementation references:

- [Chromium serialized storage keys](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/common/storage_key/storage_key.cc)
- [Firefox origin attributes](https://raw.githubusercontent.com/mozilla-firefox/firefox/main/caps/OriginAttributes.cpp)
- [Firefox partition ancestor semantics](https://raw.githubusercontent.com/mozilla-firefox/firefox/main/toolkit/components/antitracking/StoragePrincipalHelper.cpp)
- [Firefox persistent Cookie schema](https://raw.githubusercontent.com/mozilla-firefox/firefox/main/netwerk/cookie/CookiePersistentStorage.cpp)
- [Firefox BiDi storage implementation](https://raw.githubusercontent.com/mozilla-firefox/firefox/main/remote/webdriver-bidi/modules/root/storage.sys.mjs)
