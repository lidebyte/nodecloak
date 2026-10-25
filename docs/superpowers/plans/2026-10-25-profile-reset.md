# Browser profile reset implementation plan

**Goal:** Add a confirmed one-click environment reset that restores default settings, generates a fresh fingerprint, clears browser data, and preserves the proxy unless explicitly selected.

**Spec:** User confirmed clearing Cookie/cache/login data, not just configuration. Preserve profile identity, browser, name, tags and notes. Default proxy checkbox is off. Do not execute reset against any real user profile during validation. No new release requested.

**Architecture:** Add a focused child module under profiles for a closed-profile reset journal, browser-directory staging and recovery. Keep proxy credentials/settings unchanged by default. Track pending session resets outside the browser directory so cloud reconciliation cannot silently restore old login data. Use a dedicated VI confirmation dialog and existing mutation lock.

**Files:** `src-tauri/src/profiles/reset.rs`, profile loader/command registration, cloud session reconciliation, `src/ProfileReset.tsx`, BrowserProfiles menu, frontend bridge/demo and English translations.

- [x] Write failing backend temporary-profile and frontend confirmation regressions.
- [x] Implement bounded journal/recovery, ownership and native lock guards, fresh settings and optional proxy reset.
- [x] Protect reset session state from automatic cloud import; retain explicit conflict resolution for concurrent remote edits.
- [x] Implement menu, accessible confirmation, default unchecked proxy option, loading/error feedback and VI reuse.
- [x] Run focused red/green, full frontend/Rust/build checks and independent review; visually inspect if a safe local preview is available.

**Verification:** All three browser IDs, legacy and named profiles; actual synthetic cookies/cache removal; other profile preservation; proxy/password reference/API/chain retention and optional clearing; fresh stable fingerprint; live lock and shared-profile rejection; restart recovery; cloud reset marker retention/acknowledgement and no silent import; cancel/escape/double-click/failure/reopen checkbox behavior.

## Evidence

- The missing menu/confirmation regression failed first, then passed. Native preservation/default-proxy tests failed against the unimplemented command, then passed. A separate remote-session regression failed before the reset import guard was added. The cache invalidation regression likewise failed before targeted invalidation.
- Independent review found a recovery race through the sharing watchdog's read path. A dedicated mutex now covers normal registry reading and reset recovery. The watchdog and its asynchronous close helper use a validated, non-initializing, non-recovering snapshot loader, so large cleanups cannot delay expiry enforcement. Its regression failed with the blocking loader and passed with the snapshot path. Concurrent reads, all staging boundaries, corrupt/ambiguous records, and interrupted Windows cleanup are covered.
- Local Windows Rust suite: 286 passed / 37 ignored. Frontend: 200 tests / 47 files passed. TypeScript/Vite production build passed; existing bundle-size warning remains. No new macOS runner result is claimed for this feature.
- Synthetic HTTP round trips verified encrypted empty-session publication at the expected config revision, acknowledgement only after success, retention after lease failure, and preservation of local reset state when a newer remote session requires an explicit decision. Synthetic credentials were removed, including a leftover from the initial failing fixture.
- Isolated browser demo at localhost:1431 visually verified the VI dialog, unchecked proxy default, changed explanation when selected, initial Cancel focus and cancellation. No reset was executed through the preview, and no real user profile was modified. Screenshot: .qa/profile-reset-preview.png.
- Review issues are resolved; no remaining important blocker reported. No version bump, installer publication or production deployment performed.
