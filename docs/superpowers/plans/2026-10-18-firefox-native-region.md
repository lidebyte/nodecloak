# Firefox Native Regional Configuration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve Firefox manual regional settings without starting a WebDriver session.

**Architecture:** Use native Firefox browsing-context timezone/locale fields and the existing native geolocation provider. A temporary loopback DevTools setup connection configures browser-owned observers, then closes its listeners before handing off to web content. The approved privacy extension remains unchanged; MCP remains explicitly automated.

**Tech Stack:** Rust, Firefox RDP/native browsing contexts, existing signed WebExtension, Vitest, installed-browser fixtures.

**Spec:** `docs/engineering/firefox-manual-fingerprint.md`, plus the user's requirement to fix custom regional compatibility.

## Global Constraints

- No webdriver property overrides, Google-specific scripts or disabled browser signature checks.
- Preserve seed, login data, proxy routing and explicit regional choices.
- IP matching must use the launched Firefox network stack; failure must not release the destination or substitute real device location.
- No edit to the approved extension payload without a new approved artifact.
- Record cross-platform runtime limits accurately; no automated account credential entry.

### Task 1: Prove the native configuration path

- [x] Reproduce current manual regional routing tests failing when no WebDriver is expected.
- [x] Disposable native Firefox: native timezone/locale, winter/summer offsets and Worker agree, webdriver=false.
- [x] Close DevTools listener; verify OS listener absence and cross-origin COOP navigation retaining settings.

### Task 2: Integrate startup and lifecycle

Files: new `firefox_region.rs` / `firefox_region.js`; modify `firefox.rs`, `platform.rs`, `profile_commands.rs`, `firefox_extension.rs`, `regional.rs`, `browser-profiles.ts`.

- [x] Add bounded RDP framing and fixed native configuration scripts.
- [x] Restore temporary debugger preferences and close listeners before navigation; recover interrupted setup.
- [x] Resolve IP through Firefox, configure native location/locale/timezone/viewport and retain web startup readiness checks.
- [x] Route all manual Firefox regional settings independently of MCP and noise switches.

### Task 3: Verify and review

- [x] Test actual app launch: custom region, IP region, restart, disable/reset and browser-local destinations; separate native fixture covers Worker/popup/COOP, existing extension fixture covers iframe markers.
- [x] Test failed IP initialization and stale debug setup recovery without exposing target pages.
- [x] Run Rust/frontend/background checks; request independent code review and resolve important findings.
- [ ] Compile repaired desktop executable; user manually verifies Google and Claude in actual App profile.
- [x] Update internal evidence, including Firefox-version and OS coverage limits.
