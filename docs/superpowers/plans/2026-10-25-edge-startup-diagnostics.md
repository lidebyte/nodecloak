# Edge startup and accessible profile diagnostics

**Goal:** Fix the local Edge endpoint timeout and expose profile startup logs directly in the app.

**Architecture:** Diagnose the installed Edge with disposable profiles, preserve the endpoint ownership guard, and add a bounded read-only diagnostics command and VI-consistent dialog linked from errors and profile menus.

**Constraints:** Preserve user profiles, login data and proxy credentials; do not bypass endpoint ownership or site security. No release requested in this task.

- [x] Reproduce installed Edge startup and establish why the endpoint is missing.
- [x] Add a failing regression and fix the demonstrated startup incompatibility.
- [x] Add bounded, guarded profile-log reading and tests for missing/rotated/unsafe files.
- [x] Add error/menu log access, refresh/copy actions, bilingual labels and UI tests.
- [x] Run relevant Rust/frontend/native regressions; build the desktop test executable and report verified behavior and remaining limits.

## Evidence

- Installed Edge 155.0.4283.45 exposes the fixed-port endpoint but exits the
  launcher and closes stderr without the DevTools line. The old app-path native
  test reproduced the endpoint timeout; the corrected test passes and confirms
  navigator.webdriver remains false. The fallback verifies exact profile PID
  ownership of the IPv4 loopback listener before and after its bounded request.
- Rust: 269 passed / 37 opt-in checks ignored. Frontend: 192 passed / 45 files.
  Production frontend and embedded desktop builds passed. Independent review
  found no critical or important issue. Focused refresh/copy and error/menu
  diagnostics tests pass.
- The user exited the installed app after saving. The embedded repair app was
  opened through the Windows shell and loaded the existing Losangels profile.
  Actual profile log now records endpoint_handoff, endpoint_published and
  connected, with about 0.7 seconds for endpoint discovery; its Edge window is
  displaying the default IP.NodeCloak page. The user's concurrent UI activity was
  respected rather than continuing competing inputs.
- This is a local repair build, not a published update. No profile or proxy
  configuration was changed to bypass the failure. Private test evidence lives
  under .qa/edge-startup-20261025.
