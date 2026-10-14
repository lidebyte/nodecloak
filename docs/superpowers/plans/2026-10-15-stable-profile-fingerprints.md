# Stable Profile Fingerprints Implementation Plan

**Goal:** Generate independent fingerprints for new Chrome, Edge and Firefox profiles, retain them across restart, expose regeneration and configurable deterministic readback noise.

**Architecture:** Persist a versioned 256-bit fingerprint seed and five independent protection controls in profile preferences. Install one audited readback script through native CDP/BiDi before website scripts. Keep real UA/GPU/hardware identities and existing coherent regional controls. Noise is deterministic per profile/origin; it changes reads rather than stored website data. Native Firefox randomization must not double-randomize stable mode.

**Tech stack:** Rust/serde/getrandom, CDP/BiDi, JavaScript API wrappers, React/VI controls, native installed-browser fixtures.

**Constraints:** Preserve previous uncommitted changes. No deployment/version bump or commit requested. Test with synthetic temporary profiles. Older saved profiles keep existing behavior until opted in. New profiles default to protected settings. New configuration schema must be rejected by old clients. Do not hide automation markers, alter HTTP identity, synthesize fake media hardware, bypass permissions or claim account-verification outcomes. Firefox BiDi preload applies to windows/iframes; worker coverage must be reported honestly.

## Tasks

- [x] Persistence: failing regression creates two profiles, compares 64-hex seeds, reloads one and confirms stability; implement `fingerprint::Settings`, validation, native seed generation, new-vs-import creation policy, config schema 3, encrypted round-trip and malformed-input/legacy tests.
- [x] Readback: failing executable fixtures verify repeat reads, distinct seeds, nonmutation and disabled modes for Canvas/font metrics, WebGL pixels, offline audio and media pseudonyms. Implement a bounded, deterministic script; preserve API errors, object types, permission semantics and exact device selection.
- [x] Runtime: inject CDP Page preloads and Worker scripts before debugger resume; install Firefox global BiDi preload before first navigation. Resolve native Firefox randomized protection conflicts. Installed-browser tests verify first page, iframe, new tabs, restart, independent seeds and compatibility behavior.
- [x] UI: add a VI fingerprint card with master switch, per-surface noise controls, short identity and regeneration confirmation. Existing save persists the new seed; cancellation does not. New defaults use crypto randomness; old-profile normalization preserves old behavior. Add component, preview and lifecycle tests with real rendering.
- [x] Verification: run native, frontend, executable-script and installed-browser checks; independent code review; document exact context coverage and limits; confirm the local development frontend is available and the native library compiles.
