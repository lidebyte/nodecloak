# MCP and profile settings separation

## Evidence and scope

The parent of commit `481b70d` (MCP introduction) already enables the browser control path for stable fingerprint settings and custom timezone. This rules out attributing that dependency solely to MCP. User reports correlate login rejection with these settings; they do not identify the specific signal responsible for rejection.

This change separates launch intent, command admission, settings initialization and event handling. It does not replace the fingerprint provider or claim that a third-party login issue is fixed. No browser kernel, website-specific exception, automation concealment flag or UI mode switch is introduced.

## Behavior

| Launch | Control-dependent settings | Outcome |
| --- | --- | --- |
| Desktop | Off | Existing native browser launch; no MCP controller |
| Desktop | On | Settings controller only; MCP page commands rejected |
| MCP profiles_start | Off | Automation controller; no injected fingerprint/region emulation or Worker interception |
| MCP profiles_start | On | Both independent capabilities enabled, preserving saved settings |

MCP cannot silently upgrade a running manual session. It returns `RESTART_REQUIRED`, leaving the user's browser running. The user closes that session before calling MCP `profiles_start`. Every MCP operation still passes existing grant, origin, approval, expiry and file policy checks. Reconnect retains the launch capabilities. A desktop restart uses manual intent again. This intentionally changes the previous behavior where any live controller could be reused for MCP.

The native protocol remains a shared implementation detail. Removing CDP/BiDi entirely from manual stable fingerprints and timezone settings is a separate provider/kernel project.

## Verification

- A wire-level regression test first reproduced unconditional MCP prompt subscription during Firefox settings initialization.
- Protocol tests cover Chrome, Edge and Firefox: manual event isolation; MCP-only absence of emulation/preload/Worker pause commands; preservation of explicitly configured fingerprints/timezone; admission rejection before dispatch.
- Firefox preference regression checks that automation-only upload and first-run preferences are cleared for manual launch.
- Native opt-in tests cover both stable fingerprint enabled/disabled MCP workflows, plus manual fingerprint/timezone startup in all three installed browsers. Their results are recorded after execution below.

## Results (Windows, 2026-10-15)

- `cargo test --manifest-path src-tauri/Cargo.toml --lib --quiet`: 182 passed, 22 opt-in tests ignored, no failures.
- `native_mcp_page_operations_all_browsers --ignored`: passed all six combinations (Chrome/Edge/Firefox × fingerprint off/on), including input, click, selection, navigation, screenshots, upload/download, dialogs, cross-origin frames and stale-reference rejection.
- `native_manual_fingerprint_startup_is_separate_from_mcp --ignored`: final all-browser run passed. Observed first-document/iframe fingerprint marker and main/iframe/Worker timezone; Chromium Worker fingerprint marker also verified. Firefox Worker fingerprint injection is not claimed by this test.
- The initial manual test reused a geolocation fixture despite blocking location; Firefox correctly removed that API and the fixture failed. It was replaced with a dedicated fingerprint/timezone fixture.
- One subsequent Chrome run returned an empty Worker error report while the main page and iframe passed. No production fix was made for this observation. After adding test-only initialization diagnostics, a Chrome-only rerun and the complete three-browser rerun passed. This remains an intermittent observation, not a proven resolved Worker issue.
- `git diff --check` passed. Tests used temporary profiles and local fixture pages. No Google/Claude authentication or Cloudflare challenge acceptance was verified; no macOS/Linux native run was performed.

## User follow-up: Edge startup and Chrome login

The user reported that testchrome still fails login and testedge reports `网站来源无效`. Read-only inspection of testedge's QuotaManager found HTTP(S) website storage partitioned under `edge://copilot-ntp` and `edge://discover-chat-v2`. Discovery passed those internal top-level sites to the web-only origin parser and aborted session preparation before the browser launch.

Local storage discovery now excludes well-formed browser-internal partitions from portable website sessions, without deleting their source data or flattening them into website state. Imported snapshots still reject non-web origins, and malformed or unsupported web namespaces still fail validation. This restores the specific discovery fix that had been removed by the earlier user-requested full rollback; unrelated stashed changes remain untouched.

Validation: the new synthetic quota/LevelDB regression reproduced the reported error before the fix. After the fix, 184 ordinary Rust tests passed. An installed Edge integration test passed export of synthetic website state alongside an internal Copilot partition and verified the original internal metadata was retained. The desktop watcher compiled and started the updated executable successfully.

testchrome's saved settings currently have stable fingerprint disabled and custom timezone `Europe/London` enabled. Custom timezone still requires the existing control transport. The user's continued login failure is retained as unresolved; this observation does not establish which signal Google or Claude rejects. The MCP separation and Edge discovery repair do not establish login compatibility.

### Follow-up: actual Edge capture disconnect

A read-only-source diagnostic launched only a staged copy of testedge, with external network requests intercepted as in the existing export pipeline. Website namespace discovery and cookie retrieval completed. Two runs reproduced a WebSocket connection reset (`Page.navigate`, Windows 10054) while navigating the staged target to the `ntp.msn.com` namespace. The browser child had not reported an exit at the moment of failure. This narrows the failing operation but does not establish the reason Edge resets the connection.

Export now retries a transport failure once using a new staged copy and records command/error class/OS code without cookie values, protocol payloads or authentication URLs. If capture still fails, only an owner's local launch can continue with the original profile, while preserving the sync error, pending data and baselines. It does not upload a partial snapshot, overwrite local login data, or acquire a sync lease. Explicit synchronization and received shares still fail on capture errors; configuration conflicts and invalid storage remain blocking. The capture failure itself is not represented as fixed.

Validation: the owner-launch policy regression failed before the behavior change, then passed. The current Rust suite reports 187 passed and 24 opt-in tests ignored. Retry tests cover a one-time transport failure, a persistent failure bounded to two attempts, and non-transport storage errors that must not retry. The actual staged-profile diagnostic remains a reproducible failure and is intentionally not claimed as a passing sync test. The desktop watcher compiled and launched the updated build.

Google login remains unresolved. The new bounded export handling is independent of the manual browser timezone/fingerprint control path and does not claim to address Google's rejection.

## Confirmed fixes after the latest user test

### Manual Chrome sign-in with custom timezone

The manual Chromium settings path used `--remote-debugging-port=0`, which sets Chromium's native automation indicator even without `--enable-automation`. Manual settings startup now reserves a nonzero loopback port; MCP startup retains its existing automation startup. The endpoint is taken from that launched child's stderr, strictly checked for the expected loopback port and browser route, then written atomically to the existing endpoint marker. Raw browser stderr is not persisted. No JavaScript replacement of `navigator.webdriver`, website-specific exception, or extra UI mode was added.

The installed Chrome/Edge/Firefox manual startup test passed with stable fingerprint enabled and a custom `Asia/Tokyo` timezone in the main page, iframe and Worker. Chromium manual sessions reported native `navigator.webdriver === false`. The user then restarted testchrome with the original London timezone and explicitly confirmed: “Google 已能正常登录”. This verifies the reported Google login case; it does not establish Claude login or all future third-party risk decisions.

Reference: [MDN Navigator.webdriver](https://developer.mozilla.org/en-US/docs/Web/API/Navigator/webdriver).

### Edge isolated session export

The original testedge failure was reproduced in a staged copy at the unpartitioned `https://ntp.msn.com` namespace. An empty synthetic profile reproduced it too. The browser exited with Windows exception `0x80000003`; Microsoft's public symbols locate the failing cold code within `MicrosoftURLLoaderThrottle::WillProcessResponse`. Headed startup, removing iframe auto-attach, changing the probe path and removing the top target's Fetch interception did not eliminate the failure. It is therefore recorded as an incompatibility of Edge's origin-specific response processing with the offline capture document, not as a proven user-profile corruption or a generic timeout.

The capture/import pipeline no longer needs a website document for unpartitioned Chromium localStorage-only namespaces. It reads the closed staged LevelDB directly, and writes replacement state in a synchronous batch only after the isolated browser has closed. It validates schema VERSION=1, Latin-1/UTF-16 encoding, input shape, duplicates and size limits; updates Chromium usage metadata; verifies the complete written state before the existing atomic stage commit. Unknown formats fail without replacing the original profile. No hostname is dropped or hardcoded into the production fast path.

Legacy IndexedDB directories, QuotaManager bucket ownership, partition identities and the newer SQLite LocalStorage backend determine eligibility. IndexedDB and partitioned data keep their existing browser codec. A bucket owned by another website does not force NTP document loading. The layout is checked again before writing so a backend change during browser work cannot silently redirect writes into an obsolete DB.

A real staged Chromium DB exposed a point-lookup discrepancy: rusty-leveldb returned no VERSION through `get`, while bounded iteration returned VERSION=1. Version validation therefore uses bounded iteration, as record export does. Browser API interoperability tests cover native writes read by both Chrome and Edge and browser writes read natively, including Latin-1, Chinese, emoji, empty strings and embedded NUL.

The formerly failing Edge NTP import/export regression passed. A complete read-only-source testedge capture then passed with 93 cookies and 7 nonempty portable namespaces (8 discovered, one empty), explicitly retaining NTP state. No cloud upload or original-profile import was performed by this diagnostic. Export retry remains bounded to two fresh stages and retains safe command/error-class/OS-code logs; it is not the mechanism used to make this capture pass.

Native layout references: [Chromium localStorage LevelDB schema](https://github.com/chromium/chromium/blob/main/components/services/storage/dom_storage/leveldb/local_storage_leveldb.h), [metadata protobuf](https://github.com/chromium/chromium/blob/main/components/services/storage/dom_storage/leveldb/local_storage_database.proto).

User acceptance: the saved testchrome configuration was confirmed read-only as stable fingerprint enabled with custom `Europe/London`. The user confirmed Google login succeeds. After loading the Edge repair, the user also confirmed “启动和同步都正常” for testedge.

Final verification (current Windows development build):

- Ordinary Rust suite: 194 passed, 26 opt-in tests ignored, zero failures.
- MCP native operations: all six Chrome/Edge/Firefox × stable fingerprint off/on cases passed (106 s).
- Partitioned Cookie/localStorage/IndexedDB migration, restart isolation, failed-import rollback and isolated partition deletion: all three browsers passed (210 s).
- Native localStorage/browser API interoperability: Chrome and Edge passed.
- Edge NTP storage import/export regression: passed after the final eligibility and layout-write guards.
- The MCP run logged one auxiliary Edge page target `Page.enable` timeout with fingerprint enabled. The existing fail-closed handler closed that target; tested page operations subsequently passed. No separate fix or absence of all target-initialization warnings is claimed.
- New native storage module formatting and `git diff --check` passed. The existing repository-wide formatting baseline is not asserted clean.
- Local desktop watcher compiled and started the current development executable. No version bump, commit, release, deployment or hibernation was performed in this fix task.

## Login popup follow-up (2026-10-15)

The user subsequently reported that Google sign-in opens a tab which is automatically closed. The existing Chromium target handler explicitly closes a new target after settings initialization fails. Before this fix, initialization awaited `Page.enable` (5-second timeout), and read the new target's `Intl` state before resuming it. A paused popup can withhold the enable response; the last native MCP run had also observed an Edge `Page.enable` timeout followed by this close handler.

A new disposable-profile regression opens a login popup through a user gesture, follows a cross-origin HTTP redirect and applies `Cross-Origin-Opener-Policy: same-origin`. Before the repair, its first script observed the machine timezone despite the requested `Europe/London`: the initial about:blank inherited the opener's Intl state, causing the code to skip persisting the target's own override. The native fingerprint Page agent must also remain enabled for its preload to run.

Chromium document targets now queue Page activation, preload registration, target-specific native overrides and recursive attach setup together. The controller collects successful acknowledgements for protection commands before sending `Runtime.runIfWaitingForDebugger`; only the `Page.enable` acknowledgement may remain deferred. It then waits for both remaining acknowledgements. Initialization no longer executes JS or treats inherited Intl values as proof of persistent target configuration. The worker-specific startup path and Firefox's BiDi settings path remain separate.

Wire regressions exercise a Page.enable reply withheld until resume, and rejection of either preload registration or timezone override. Rejected protection commands must never send resume. Pending navigation replies and unrelated events are retained. Independent review identified the need for this acknowledgement gate; it was added and re-reviewed with no remaining concrete findings.

The final gated native popup regression passed on installed Chrome and Edge: the first script saw the fingerprint marker, `Europe/London`, and native webdriver=false; the tab remained open beyond the former 5-second close interval. The user confirmed: “不再关闭，登录正常”. The final gated development build compiled and loaded successfully.

Protocol timing reference: [Playwright Chromium page initialization](https://github.com/microsoft/playwright/blob/main/packages/playwright-core/src/server/chromium/crPage.ts). This implementation retains stricter acknowledgement gating for protection commands.

Final popup-fix validation: 196 ordinary Rust tests passed (27 opt-in ignored). Native manual settings startup passed Chrome/Edge/Firefox, and native MCP operation cases passed all six fingerprint off/on combinations. The Edge MCP fingerprint-on case still logged an auxiliary page target initialization timeout (now reported as Emulation.setLocaleOverride); the existing handler closed that target while the tested protected page operations passed. This auxiliary warning is retained as unresolved and is not represented as proof that all target initialization failures are eliminated. The Chrome/Edge manual login-popup regression and the user's Google login acceptance are the specific evidence for this repair. `git diff --check` passed. No release/version change was made.


### Follow-up 2026-10-16: rendererless tabs and deferred initialization

The user reported that both testchrome and testedge still lost login tabs. The earlier gated repair was therefore insufficient. The Edge native MCP regression reproduced a pending empty-URL page with Page.enable, preload registration and locale override acknowledgements withheld, no queued child targets, and a 15-second destructive close. An awaited Debugger.enable probe stalled too. Chromium explicitly allows an auto-attached target to exist before renderer allocation and suspends renderer messages until navigation supplies a live host. Waiting synchronously for such a target was the architectural error.

Document initialization now keeps pending command IDs, the saved startup resume, and readiness/error state independently for every session. The shared read path routes setup replies while ordinary navigation and other targets keep progressing. Only Page.enable may acknowledge after resume; all protection commands still have to acknowledge first. A 15-second delay logs pending method names and retains the attachment and startup pause. Late renderer allocation/replies can finish setup. Neither delay nor rejection closes or detaches the target. Rejection keeps the pause and emits a sanitized per-profile diagnostic. Delayed completion emits a recovery diagnostic. No website URL, form value, fingerprint seed, session ID or raw protocol message is persisted.

Resolved configuration updates append acknowledgements to an existing provisional setup gate, so custom/IP settings cannot be discarded by an initialization already in progress. Detachment clears the session setup and reselects a surviving page; startup requires a live, initialized primary page. MCP frame enumeration keeps pending top-level tabs but does not request their unavailable frame trees.

Validation: ordinary Rust tests pass, including delayed replies beyond the old deadline, other commands progressing, final resolved geolocation updates, rejected protection never resuming, and detached-primary replacement. Installed Chrome and Edge preserve empty background tabs for 18 seconds before navigation; first-script beacons then observe the stable fingerprint, Europe/London and native webdriver=false. Ordinary redirect/COOP and synchronous document.write popup fixtures also passed both browsers during investigation. Final native manual startup passed Chrome, Edge and Firefox; final native MCP operations passed all six browser/fingerprint-off-or-on combinations. Independent review found two pending-generation/primary-lifecycle defects, both were reproduced with failing regressions and repaired; re-review found no further blocking defect.

One additional opt-in native geolocation regression did not pass on this occluded desktop: its main document reported visibility=hidden and position timeout, then its latitude assertion panicked. Timezone/language observations were correct. That run is not claimed as completed geolocation validation; no production change was made to alter browser visibility or geolocation policy.

Sources: [Chromium TargetHandler](https://raw.githubusercontent.com/chromium/chromium/main/content/browser/devtools/protocol/target_handler.cc), [Chromium DevToolsSession](https://raw.githubusercontent.com/chromium/chromium/main/content/browser/devtools/devtools_session.cc).

### Acceptance blocker: deleted cloud configuration

The user could not restart testchrome/testedge because of HTTP 409 config_conflict. Read-only local metadata confirmed both associated cloud configuration baselines were tombstones while live local profiles and deletion/edit conflicts remained. The user explicitly chose to leave those states unchanged and finish code fixes first. No live conflict was resolved, profile removed, cloud object restored or binding detached.

The launch guard now detects a deleted cloud config even when its revision equals the local baseline. It records a configuration deletion/edit conflict, clears the local lease generation and returns actionable guidance before capturing or uploading login data. A late concurrent config_conflict retains the existing conflict persistence and receives an actionable message. The synthetic regression initially returned success through the unsupported-storage fallback; it now stops before session work and confirms the local sentinel survives. Launch-policy tests and independent review passed. Current Google login acceptance remains pending explicit conflict resolution; previous user acceptance is not reused as proof for this follow-up.

### Loading follow-up: noopener first-document initialization (2026-10-16)

The user reported that tabs were retained but stayed loading. The earlier asynchronous acknowledgement gate still deadlocked a reproducible `window.open(url, '_blank', 'noopener')` popup: Chromium's browser-side startup pause prevented navigation/renderer allocation, while NodeCloak waited for renderer-side Page/preload/locale/timezone replies before releasing that pause. Ordinary opener-preserving and document.write popups did not expose this interleaving.

The delayed-target path now queues a native `beforeScriptExecution` instrumentation breakpoint before releasing browser-side startup. This lets navigation allocate the renderer while holding the first page script until the queued protection commands acknowledge. The normal responsive-target path retains its existing acknowledgement gate. Once setup succeeds, cleanup removes the breakpoint, resumes the native instrumentation pause through an IO command, then disables the debugger through its main-thread command. Only one page initialization barrier may be active at once, because debugger pauses can span sessions sharing a renderer. Rejected protection retains ownership; a second stalled target cannot resume it. Firefox's separate BiDi path is unchanged.

A desired-settings update during any active barrier is deferred into a new setup generation. Sending main-thread overrides into an instrumentation pause would deadlock even if the original command replies had already been sent but not read. The cleanup starts that next generation before exposing readiness, and startup waits for final settings before user navigation. Wire regressions were observed failing for both the release phase and the earlier Arming interval before this deferral was completed.

A native negative regression submits an actual invalid preload source type in disposable Chrome and Edge profiles. Both browsers acknowledge rejection and emit the instrumentation pause; the first page-script beacon remains absent and the tab remains open. This verifies protection-command failure with a successfully installed native barrier. It does not claim that native debugger installation rejection, external debugger interference, or transport detachment can preserve a browser-owned pause: static supported debugger commands are used, installation errors are reported, and no unconditional protection guarantee is made for those protocol failures.

Validation completed on the final logic:

- Ordinary Rust suite: 201 passed, 28 opt-in tests ignored, zero failures.
- Native Chrome/Edge noopener + cross-origin redirect + COOP popup: passed; first inline script reads stable fingerprint marker, Europe/London, and native webdriver=false, then an eight-second heartbeat confirms the tab remains alive.
- Native Chrome/Edge ordinary opener and synchronous document.write popup variants: passed with the same first-script checks.
- Native Chrome/Edge actual rejected-preload protection regression: passed.
- Native manual startup: Chrome, Edge and Firefox passed main/iframe/Worker timezone and supported fingerprint checks; manual Chromium webdriver remains false.
- Independent review identified the deferred-generation phase cases above; both were repaired and final review found no additional concrete blocker within the stated native guard-install limitation.

Final MCP regression passed all six Chrome/Edge/Firefox × stable-fingerprint-off/on cases (90 s): click, fill, select, file upload, dialog, download, screenshot, child-frame navigation and stale-reference rejection. `git diff --check` passed. The desktop watcher compiled the final code and the running process was verified as the development executable under the configured useclaude target directory. No version change, release, commit, deployment or hibernation was performed.

The previous Google acceptance is historical. Current user Google/Claude sign-in acceptance is still pending a restart on this build. The previously identified cloud tombstones and conflict choices remain unchanged; this repair does not restore, delete, detach, or upload those profiles.


### Release review: short-lived Worker lifecycle

Pre-release static review found synchronous Worker initialization still propagated a confirmed TARGET_GONE into controller recovery. The new regression reproduced this failure. Worker setup now permits the already-queued detach event to reach common target cleanup at any initialization phase, without closing other tabs or suppressing genuine command/transport failures. Four stages are covered: Debugger.enable, locale override, nested auto-attachment, and startup resume. Final ordinary Rust suite passes 222 tests (30 opt-in ignored).
