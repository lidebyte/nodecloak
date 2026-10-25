# Firefox offline session compatibility

**Goal:** Prevent profile extensions in temporary offline session clones from cancelling storage navigation and blocking normal browser-profile startup.

## Evidence and cause

The reported 3.2.0 startup log repeatedly contains `session_capture_failed` for `browsingContext.navigate`. A read-only staged diagnostic with Windows Firefox 158.0.1 reproduced failure on the first of six namespaces: BiDi remained connected, the page stayed `about:blank`, and the safe protocol category was `unknown error / NS_ERROR_ABORT`.

The offline clone includes the installed profile privacy companion. Its native host rejects the temporary clone identity, so its blocking navigation listener cancels documents. Removing extensions from only that temporary clone allowed all six namespaces to navigate. The original profile was never launched or edited for this diagnostic.

## Change

- [x] Isolate Firefox extension packages, `extensions.json` and `addonStartup.json.lz4` outside the offline browser tree, suppress external extension discovery there, and restore original packages, metadata and prefs before transactional import commit.
- [x] Keep the normal browser profile and its fingerprint extension unchanged. Do not modify the embedded companion package or weaken its startup checks.
- [x] Preserve allowlisted protocol error codes and Firefox navigation categories in diagnostics without including website URLs or arbitrary browser messages.
- [x] If offline navigation capture still fails, permit an owned profile's local launch without uploading partial data, importing remote data or taking a cloud lease. Existing sharing and conflict checks remain in force.

## Verification

- Three regressions were observed failing before implementation: extension isolation/restoration, safe protocol diagnostics, and owner-only local startup handling.
- 82 cloud tests and the full 288-test Rust suite passed; 37 opt-in browser checks remain ignored by the ordinary suite.
- The reported local browser data exported successfully after the fix, covering all six nonempty website namespaces. All 1,607 original file hashes remained unchanged.
- Native Firefox protected-web storage import/export passed, including exact preservation of companion package/metadata and extension local-storage/settings sentinels. No real website scripts were loaded by these offline tests.
- Independent review found no blockers. Follow-up Mozilla source review confirmed that hiding both metadata stores avoids treating those hidden profile addons as prior uninstall candidates. Native preservation assertions cover the tested addon-state paths; broader browser/platform testing is not implied.
- The desktop repair program compiled and its running executable/window were verified. On 2026-10-26 the user confirmed the repaired profile starts successfully and requested a patch release. Release tracking continues in `2026-10-26-release-3.2.1.md`.

Detailed, private repro logs, file-hash evidence and repair executable are in `.qa/firefox-session-navigation/`.
