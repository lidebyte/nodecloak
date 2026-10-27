# Large login snapshots

Login synchronization supports an uncompressed serialized snapshot up to 64 MiB per profile, containing cookies and supported LocalStorage/IndexedDB state. It does not silently remove websites, records or login state to fit a size limit.

## Compatibility and wire format

Plaintext at or below 8 MiB continues using the existing v1 encrypted envelope. Larger plaintext uses v2: a separately authenticated domain, a 13-byte encrypted frame header (`NCS2`, codec 0/raw or 1/gzip, little-endian u64 uncompressed byte length), and the encoded bytes. Gzip is selected only if smaller than raw. Both data and wrapped-key authentication bind the v2 domain to the existing account/object/kind/revision/epoch/writer/generation identity. Readers support v1 and v2; older clients reject v2 without importing partial state or advancing their baseline.

Authenticated decryption precedes decompression. Decoded output must exactly match the declared length and may not exceed 64 MiB. Unknown codecs, truncated/corrupt gzip, additional members, trailing bytes and oversized lengths fail before browser mutation. The native browser restore still uses the existing staged transaction and readback validation.

## Capacity boundaries

- Raw serialized snapshot/plaintext: 64 MiB.
- Opaque object/share request, response and encrypted local cache: 96 MiB, enough for an incompressible 64 MiB frame plus encryption/Base64/JSON overhead.
- Isolated browser protocol: 144 MiB, accounting for Firefox's additional JSON string escaping.
- Existing node/record/depth/count checks remain enabled; browser state aggregate limits use UTF-8 bytes.
- Account object and share quotas remain 256 MiB of opaque payload per existing server rules. Compression reduces usage; the cap is not a promise of unlimited cloud storage.
- Potentially large or unknown-size snapshot requests and session/share payload reads are limited to two concurrent materializations per service process; known-small writes keep normal concurrency. Saturation returns the existing retryable `try_later` response.

## Rollout

Deploy the compatible server changes and updated `server/deploy/nginx-account.conf` before rolling out clients that can upload incompressible large snapshots. Session/share payload limits are 96 MiB; configuration payloads retain their existing 12 MiB bound, rotation retains its 360 MiB aggregate request bound, and the `/v1/` proxy body/read/send limits must accommodate these existing aggregate transactions. Data-only native HTTP and server deadlines are 180 seconds; ordinary native API requests and general server deadlines remain 30 seconds.

All devices that need to receive a v2 snapshot must update to a client with v2 support. Small snapshots remain compatible with older clients. If an old server rejects a larger upload, the new client reports that the service must be updated and preserves local data.

This implementation introduces no schema migration, plaintext server-side storage or new key requirements. Release publication and live deployment are separate from development verification.
