# Stable profile fingerprints — internal verification

This local change does not publish or bump a release.

## Identity and editing

New local Chrome, Edge and Firefox profiles receive a versioned, cryptographically generated 256-bit seed. Local configuration copies receive a new seed; cloud restoration and received shares retain the source seed. Old saved profiles without this field keep their previous behavior until enabled. Ordinary launches retain the seed.

The advanced editor provides a master switch, five independent controls, a short identifier and a regeneration confirmation. Cancellation retains the draft. Confirmation changes only the draft seed; Save persists it and a subsequent launch applies it. Regeneration does not remove cookies or website storage. Running profiles retain their current settings until restarted. Regional controls keep their own behavior.

Portable documents with fingerprint settings use schema 3. Older clients reject them instead of dropping the identity. Tests cover local-copy independence, cloud preservation, encrypted round-trip, legacy absence, invalid seeds/versions and retained local data during regeneration.

## Readback coverage

| Surface | Protection | Compatibility boundary |
| --- | --- | --- |
| Canvas | Deterministic RGB perturbation for sRGB opaque pixels, direct reads and copied exports | Source canvas stays intact; transparency, wide-gamut/high-precision exports and images above 4M pixels retain native paths |
| Font measurements | Small deterministic width offset on native TextMetrics | Real installed fonts and native object branding remain |
| WebGL pixels | Deterministic opaque RGBA readback perturbation | Tightly packed, supported byte/float reads of the default drawing buffer only; other layouts, custom framebuffers, PBOs and invalid/outside rectangles remain native |
| Audio | Deterministic sparse offline-render and float-analyser perturbations | Normal playback buffers, native sample counts and untouched tails remain |
| Media inputs | Stable aliases for native camera/microphone device and group IDs | Permission-hidden IDs remain hidden; constraints translate back to real inputs; native devices, labels and speaker IDs remain |

Noise depends on the profile seed, effective origin and API surface, never on launch time or Math.random. Origin is resolved at read time because Firefox may preload a Window before navigation commits. Work is bounded. Browser, font, hardware and origin changes can change native inputs to the same seed; this is not a complete simulated machine. UA/GPU identities, automation markers and website permissions retain their native behavior.

Chrome and Edge install CDP page/iframe preloads and protect controlled Workers before their first script. The debugger is disabled while still paused, avoiding a race when Workers close immediately. Internal browser extension Workers are excluded. Firefox uses BiDi Window/iframe preloads; Firefox Worker noise is not covered and the editor states this explicitly. The native Worker fixture covers classic dedicated Workers; shared/service/module Worker behavior is not independently verified.

Firefox's native randomized fingerprint modes are mutually exclusive with stable mode. Stable mode also replaces only the baseline EfficientCanvasRandomization target so the native session-random PNG chunk does not rotate after restart. Other baseline targets remain. Original baseline override strings in user.js and prefs.js are backed up, size/structure/path checked and restored when stable mode is disabled. That managed field uses snapshot ownership: external edits made while stable mode is active are replaced on reapplication.

## Verification

- Frontend: 137 tests in 32 files; TypeScript/Vite production build passes. Existing bundle-size warning remains.
- Native: 138 regular library tests pass; 15 external checks remain opt-in. The installed-browser fingerprint, privacy and regional suites were explicitly run and pass for all three browsers. Locked native library compilation passes.
- Executable production-script fixtures: 16 checks pass, covering media constraint arrays/inheritance/snapshots/reload, speaker routing, Canvas coordinates/range errors/transparency, WebGL padding/errors, analyser tails, TextMetrics identity and wide-gamut export passthrough.
- Installed Windows Chrome 155.0.8059.40, Edge 155.0.4283.45 and Firefox 158.0: first document, same-origin/cross-origin iframes, repeated reads, PNG/blob exports, new tabs, same-profile restart and independent seeds pass. Chrome/Edge dedicated Workers include self-close followed by further navigation. All use synthetic temporary profiles and local origins.
- Playwright: light/dark and narrower editor rendering, six VI switches, master-disable behavior, short identity, nested regeneration confirmation, cancellation and confirmation pass in an isolated context without page errors. No user profile was edited by this check.
- Independent read-only lifecycle/API reviews identified and verified fixes for media selection, WebGL packing/errors, Canvas coercion/transparency and Worker lifecycle. Wide-gamut/high-precision exports deliberately retain their native path.

Physical camera/microphone acquisition and permissions were not exercised; mapping semantics use native-like fixtures. macOS and Linux installed-browser readback has not been verified on this Windows host. Browser-environment and partitioned-session-storage validation documents cover those separate features.

## Primary references

- [Canvas API](https://html.spec.whatwg.org/multipage/canvas.html)
- [WebGL readPixels](https://registry.khronos.org/webgl/specs/latest/1.0/#5.14.12)
- [Web Audio](https://www.w3.org/TR/webaudio-1.0/)
- [Media capture](https://w3c.github.io/mediacapture-main/getusermedia.html) and [audio output](https://w3c.github.io/mediacapture-output/)
- [BiDi preload scope](https://w3c.github.io/webdriver-bidi/#command-script-addPreloadScript)
- [Firefox baseline targets](https://github.com/mozilla-firefox/firefox/blob/main/toolkit/components/resistfingerprinting/RFPTargetsDefaultBaseline.inc) and [Canvas export randomization](https://github.com/mozilla-firefox/firefox/blob/main/dom/canvas/CanvasRenderingContextHelper.cpp)
