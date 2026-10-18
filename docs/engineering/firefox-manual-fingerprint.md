# Firefox manual fingerprint delivery

2026-10-18. Internal pre-release engineering evidence, not release notes.

## Confirmed difference and implementation

Firefox 158 starts Remote Agent automation state with --remote-debugging-port,
even before a BiDi connection or fingerprint preload. The old app used this
path for stable fingerprinting, including when all five noises were OFF.
Google's internal rejection rules are private; the startup difference is proven,
while attributing every rejection exclusively to that difference is inference.

Manual Firefox now uses the companion extension when actual noise is requested.
All-noise-OFF retains its seed without injection. Explicit MCP remains automated.
Custom timezone, IP-matched language/location, custom location and viewport use
a separate native regional setup, without starting Remote Agent or WebDriver.
Native Firefox hardware limits remain supported. Actual App Google/Claude login
still requires the user's final manual check.

- extensions/firefox: signed code identical for every profile; same existing
  fingerprint.js source; strict typed settings through Native Messaging;
  MAIN-world document_start/all-frame scripts for HTTP(S). Document requests
  wait for registration and cancel on initialization failure. No remote code
  or browsing/account data collection.
- firefox_native_host.rs: one bounded get_settings request, fixed settings file
  in NODECLOAK_FIREFOX_PROFILE, strict version/seed/five-flag validation. No
  navigation, scripting, cookies or arbitrary filesystem query API. Dispatch
  occurs before Tauri starts.
- firefox_extension.rs: embedded SHA-256-pinned package, per-user native host
  registration, closed-profile permanent install/uninstall through Firefox's
  AddonManager. The installation-only about:blank instance uses Marionette then
  quits; temporary preferences are restored before ordinary native browsing.
  Signature enforcement and native navigator.webdriver are not modified.
- Web startup uses a tokenized loopback readiness page requiring the injection
  marker and native webdriver=false before releasing the destination. Failure
  gives an actionable startup error. Browser-local/file destinations must launch
  directly: HTTP content cannot navigate to them. The companion protects web
  documents, not browser internals or file documents (including the existing
  local diagnostic HTML). Disabling the companion during a session is not
  continuously monitored by this startup gate.
- Disabling noise or selecting MCP removes the companion before that path.
  Native regional settings can coexist with the companion. Re-enabling noise reinstalls using
  the same seed. Normal same-data-root app updates refresh the host executable.
  Windows host registration binds to the first data-root; separate data-root
  migration/coexistence needs explicit handling. Foreign registrations are not
  overwritten. macOS/Linux use per-user Firefox native manifest locations.

## Native regional startup

- firefox_region.rs/js temporarily opens Firefox's loopback RDP setup interface
  on an about:blank startup instance. It waits for a real browser tab before
  binding the parent target, then uses native browsing-context language/timezone
  overrides and viewport dimensions. Capability checks reject missing native
  setters before writes; no web Date/Intl or webdriver properties are patched.
- IP matching fetches through the launched Firefox network/proxy path. Response
  size, duration and returned fields are bounded. Lookup or setup failure keeps
  the destination gated, closes the owned setup process and restores preferences.
- Native geolocation uses the browser provider and preserves site permission
  checks. Its original preferences are backed up and restored on regional reset.
- Native observers cover new browsing contexts before their embedder exists,
  then apply viewport dimensions when the embedder becomes available. Tab and
  window listeners cover remoteness changes and additional windows.
- The final handoff queues browser-owned navigation, closes all temporary RDP
  listeners, and restores original debugger preferences. The web readiness gate
  verifies native webdriver=false and the expected noise state before release.
  A validated backup recovers interrupted setup at the next closed-profile launch.
  The signed extension payload is unchanged by this regional implementation.

## Approved artifact

User created the Mozilla account, personally completed 2FA and explicitly
accepted the distribution/review agreements and unlisted submission. Approval
and download verified 2026-10-18. No signing credentials are stored in source.

- ID fingerprint@nodecloak.com, version 0.1.0, desktop minimum Firefox 140.
- Unlisted slug f2d17213ec754c3fad48, version 6590875, file 5135003.
- Developer page: https://addons.mozilla.org/en-US/developers/addon/f2d17213ec754c3fad48/versions/6590875
- Submitted ZIP SHA256: 0a3832e15e3730dc35a0780f85eb2e0982859bcdc14326bdf4aa42dd05f668f9
- Approved XPI SHA256: a049892f7e3c27ffb748251612dae722f6bb0172cbd4d6342c885db4df080be2
- Embedded: src-tauri/resources/firefox/nodecloak-fingerprint-0.1.0.xpi.
- JS/LICENSE bytes match submitted source; manifest is semantically identical.
  Mozilla added signature files. Real Firefox signedState=2, active permanent
  app-profile installation. Source changes require a new signed package.
- Final desktop-only manifest lint: zero errors, one Android minimum-version
  warning for data_collection_permissions. Android explicitly unchecked in AMO.
  No Android compatibility claimed. Reviewer notes explain the native host,
  strict settings and lack of remote code/data collection.

## Verified and remaining

2026-10-18 startup follow-up: actual test6 (without native regional setup)
opened successfully, while test5 (custom timezone) remained blank after setup
listener teardown and hit the readiness timeout. The previous error incorrectly
attributed every missing request to a disabled extension. Mozilla browser-init.js
loads its command-line URI during delayed startup, after a tab already exists;
uriToLoadPromise may further defer that load. A deterministic regression showed
the old configuration admitting this state before Firefox's later about:blank
could replace the gate. Native startup now waits for delayedStartupFinished,
initial-URI settlement and a main-thread turn, then captures the settled browser
for handoff. No extension payload was changed. A native headless fixture with a
deliberately delayed initial URI passes, including real geolocation, Worker,
popup/COOP navigation and listener closure. Focused Rust: 25 passed / 6 ignored;
Node extension/regional checks: 6 passed. Full Rust library follow-up: 261 passed,
36 ignored (49.11 seconds). User verified the actual test5 profile no longer
times out with stable fingerprinting and its saved custom timezone. Its local
startup log at 2026-10-18 10:24 UTC confirms native_region_configured,
native_region_handoff, native_host_ready, readiness_page_requested,
readiness_ready and native_region_ready in sequence; the startup gate accepted
the protection report and released the destination in about 3.2 seconds.
This confirms the startup regression fix in that profile. Actual Google/Claude
login compatibility is a separate manual check and is not inferred from startup.

Startup diagnostics now distinguish host start/validation, waiting for chrome
startup, regional configuration/handoff, readiness-page GET and accepted report.
Timeouts distinguish a missing page request from a missing report. All diagnostic
details are fixed stage descriptions without destination URLs, seeds or payloads.

- Rust library: 261 passed, 36 opt-in ignored. Vitest: 190 passed / 44 files;
  tsc -b passes. Windows repair executable compiles. Routing regressions were
  observed failing on the old predicate before passing with the correction.
- Native host tests cover strict shape, malformed/oversized messages and
  disabled/future settings. Background VM tests cover gating and registration.
- Actual Windows Firefox 158 app path in a disposable profile: enabled,
  restart, disabled, enabled again all pass. Main/cross-origin frame markers
  follow the switch; native webdriver=false; no controller; unchanged seed;
  persistent site cookie survives all transitions. Synthetic test data only.
- Regional App-path integration also verifies custom LA timezone (winter/summer
  offsets), German locale/header, viewport and hardware limits; IP-matched Tokyo
  region; provider failure without destination release; reset to system; cookie
  and seed persistence; browser-local destinations and debugger preference recovery.
  Background-launched visible windows report native visibility=hidden, for which
  the geolocation API times out; the test accepts only that specific timeout.
  Final full enabled/restart/disabled/enabled plus regional matrix: one opt-in
  integration test passed in 110.44 seconds using the newly compiled repair app.
- A separate active headless Firefox fixture executes the exact production
  regional script and verifies actual synthetic geolocation, Worker timezone,
  popup first-script regional values and cross-origin COOP navigation after
  teardown. OS inspection confirms no setup listener remains. No actual Google
  credentials are read or submitted by either fixture.
- Five Node checks pass: extension registration/failure gating, early chrome
  readiness, unsupported native setters and pre-embedder popup configuration.
- Development extension checks cover first scripts, network/synchronous empty
  frames, stable canvas pixels/PNG across restart and changes with a new seed.
  Worker readbacks are not established. Mozilla documents an empty-iframe
  document_start limitation; retain the real-browser regression.
- User manually confirmed native fresh Firefox, all-noise-OFF extension, each
  noise group and ALL FIVE ON extension windows reach Google's password page.
  An earlier unlabelled rejection is contradictory evidence, not proof of a
  noise API defect. Harness did not read/submit Google credentials.
- Independent review found HTTP-to-file/about navigation in the readiness page;
  a real-browser regression covers the correction. No other important blocker
  reported. Final actual desktop-app Google/Claude manual checks remain pending;
  do not claim end-user login fixed before the result.
- macOS CI runs the native app-path regression on ARM64 and Intel, including
  first-document/frame protection, repeated starts, disabling/re-enabling,
  cookie preservation, regional signals, viewport and provider configuration.
  The hosted headless runner has no interactive macOS Location Services grant:
  only its exact permission-denied/prompt outcome is accepted, and no macOS DOM
  coordinate-delivery claim is made for that outcome. Native OS authorization
  remains unchanged in the product. Mozilla tracks this interaction at
  https://bugzilla.mozilla.org/show_bug.cgi?id=2033385.
- No Linux runtime tests or Firefox 140-through-157 coverage claimed.
  Older Firefox versions lacking native regional APIs receive an update error.
  First installation/removal may briefly open a blank setup window.

Opt-in check (build a separate executable, keeping the running app intact):

    cargo rustc --manifest-path src-tauri/Cargo.toml --bin nodecloak -- --emit=link=D:/Source/Repos/claudedone/.qa/firefox-extension/nodecloak-firefox-repair.exe
    $env:NODECLOAK_TEST_FIREFOX_NATIVE_HOST='D:/Source/Repos/claudedone/.qa/firefox-extension/nodecloak-firefox-repair.exe'
    cargo test --manifest-path src-tauri/Cargo.toml --lib native_firefox_signed_fingerprint_launch_and_disable -- --ignored --nocapture
    npm run test:firefox-extension
    python scripts/package-firefox-extension.py

References:
https://firefox-source-docs.mozilla.org/remote/Security.html
https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/contentScripts/register
https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging
https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_manifests
https://extensionworkshop.com/documentation/publish/signing-and-distribution-overview/
