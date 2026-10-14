# Partitioned Session Storage Implementation Plan

**Goal:** Synchronize persistent website login data while preserving top-level-site partition isolation in Chrome, Edge and Firefox.

**Architecture:** Extend the portable snapshot with origin/partition identities. Discover native storage namespaces from staged browser metadata, recreate each namespace in offline browser frames, and use the existing structured-clone codec. Retain staged validation, encryption, leases and conflict resolution. Unknown/opaque namespaces must still fail safely.

**Tech stack:** Rust, serde, SQLite metadata, Chromium CDP, Firefox BiDi, existing session codec.

**Constraints:** Temporary profiles only for tests. No real login values in logs. No release or deployment. Preserve existing working-tree changes. Schema 1 remains readable; partition-aware snapshots use schema 2 so older clients reject them.

## Tasks

- [x] Write and run failing tests for partition-aware snapshot validation, distinct same-origin partitions, native Chromium/Firefox namespace discovery and invalid/opaque namespace rejection.
- [x] Implement bounded storage namespace discovery and snapshot schema 2. Preserve unsupported-container/opaque fallback semantics without treating normal partitions as unsupported.
- [x] Extend the isolated browser to reconstruct top-level/iframe ancestry, intercept every frame request, and export/import each partition independently through the existing codec. Preserve partitioned Cookie scope, including Firefox native origin attributes.
- [x] Use synthetic fixtures with identical key/cookie/database names but distinct values under two top-level sites plus unpartitioned storage; verify closed-profile export/restore/restart for all three browsers and cloud snapshot encryption round-trip.
- [x] Run appropriate native/frontend/codec checks, review transactional/security behavior, and document exact support and verification results.

Results: `docs/specs/partitioned-session-storage-validation.md`. The expanded
regression also exposed and fixed current Chromium's relocated quota index,
Firefox's malformed tuple validation and fractional Cookie expiry API contract.
No real user profiles or credentials were used, and no release/deployment was made.
