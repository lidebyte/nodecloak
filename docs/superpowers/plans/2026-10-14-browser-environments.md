# Browser environment improvements

**Goal:** Make large profile collections manageable and apply verifiable privacy and regional controls to Chrome, Edge and Firefox.

**Architecture:** Keep native browser settings and CDP/WebDriver BiDi as the control layer. Navigation must process newly paused targets while waiting for acknowledgements. Batch actions call existing guarded operations and retain per-profile failures. Settings use the existing portable Preferences object and native launch path.

**Tech stack:** React, TypeScript, Rust, Tauri, Chromium CDP, Firefox WebDriver BiDi.

## Constraints

- Use NodeCloak VI, compact controls, accessible labels and bilingual copy.
- Preserve login data, received-share restrictions, trash/restore and cloud conflict rules.
- No administrator requirement for ordinary profile navigation.
- Use browser-supported settings, verify results in temporary profiles; do not advertise account-verification outcomes.
- No release or production deployment is requested in this task.

## Tasks

- [x] Navigation: add an isolated WebSocket regression where a new paused iframe blocks Page.navigate until resumed; prove failure, implement event pumping and retain actual navigation errors. Test both successful navigation and rejected URLs.
- [x] Batch management: add selection of filtered editable profiles; confirm deletion into trash; allow batch close and restore; keep failed items selected and report each failure. Test filter boundaries, running/shared exclusions, recovery and partial results against actual operation wrappers.
- [x] Regional controls: preserve explicit timezone/position sources, add system/IP/custom selectors and permission controls, show resolved preview. Test round-trip through native storage and encrypted portable documents; use existing real-browser fixtures for pages, workers and frames.
- [x] Privacy controls: separate DNT and WebRTC policy; add WebGL, WebGPU and voice exposure controls using browser preferences/launch arguments, with native capability notes. Add Firefox native fingerprinting protection while retaining regional compatibility checks. Test saved preferences and real-browser API results in all three engines.
- [x] Verify: frontend suite, native suite, production build, temporary real-browser checks, inspect UI at desktop dimensions; document tested capabilities and remaining core-dependent features.
