# Browser environment enhancements — internal verification

This change is local and does not bump the version or publish a release.

## Capabilities

| Control | Chrome / Edge | Firefox |
| --- | --- | --- |
| Language | Existing native preferences; IP/custom selection | Existing native preferences; IP/custom selection |
| Timezone | Native CDP, system/IP/custom IANA timezone | Native BiDi, system/IP/custom IANA timezone |
| Position and permissions | Native CDP coordinates and browser permissions | Native BiDi plus a profile-local inline geolocation provider for out-of-process frames |
| Logical processor hint | Native CDP in documents, iframes and Workers | Native concurrency upper-limit preference |
| Page viewport | Native CDP device metrics | Native BiDi viewport |
| DNT / WebRTC policy | Native preferences, retaining independent proxy protection | Native preferences, retaining independent proxy protection |
| Disable WebGL / WebGPU | Native launch arguments | Native preferences |
| Hide speech synthesis voices | Native Blink runtime feature | Native preference |
| Canvas protection, strict fingerprint protection, font visibility, battery API | Not added for these engines | Native Firefox settings |

Physical monitor dimensions are distinct from the page viewport. The interface labels the viewport accordingly. Stable Canvas/font/WebGL/audio/input-media readback protection is implemented separately; see [stable profile fingerprint verification](stable-profile-fingerprints-validation.md). Device-memory fabrication, arbitrary UA/GPU identities and TLS fingerprint modification are not added. Google phone-verification outcomes have not been tested or promised.

Regional and hardware controls use the browser's local automation interface. Supported regions are applied before navigating the requested page. The browser can expose `navigator.webdriver`; the interface explains use of the local control connection. Firefox hardware-only limits do not require that connection.

## Navigation and management

- `Page.navigate` now processes newly paused targets while awaiting acknowledgement. The regression fixture reproduces an iframe awaiting resume and the old timeout.
- Network navigation failures reach the UI while retaining the browser and its control connection for retry. Transport/setup/configuration failures still use the existing cleanup path. A startup network failure is retained only after controller insertion and saved runtime state succeed.
- Startup IP retries close the initial browser through its own native interface while it is still on `about:blank`. User navigation and ordinary close flows retain their existing safeguards.
- Batch selection is limited to the current filtered editable list. Received/shared profiles are excluded. Start, close, trash, restore and permanent deletion call existing guarded operations sequentially. Close/delete/purge have confirmation dialogs; failures retain their selection and report profile name and reason.
- Firefox's inline provider sends no real coordinates or Wi-Fi data to a remote location service. Original provider preferences are saved before modification and restored when returning to system positioning. Location permission is still handled independently.
- Portable documents use schema 2 for enhanced controls and schema 3 for stable fingerprint settings. New clients read schemas 1, 2 and 3; older clients reject newer schemas instead of silently dropping controls. Legacy configurations without either addition continue to export schema 1.

## Verification

Frontend: 134 tests across 30 files pass; production build passes. The existing bundle-size warning remains. Desktop and narrower editor layouts were inspected, and select menus, VI toggles, batch controls and configuration preview were exercised through Playwright.

Native: 125 regular library tests pass; 12 external/installed-browser tests are opt-in. Tests include preferences, normalization, encrypted portable round-trip, recoverable CDP/BiDi navigation failures, and provider backup validation/restoration without altering login data.

Installed Windows browsers: Chrome 155.0.8059.40, Edge 155.0.4283.45, Firefox 158.0. Real-browser fixtures use temporary profiles and synthetic local origins. macOS has not been verified in this change.

The installed-browser suite checks configured concurrency in pages/iframes/Workers, DNT, viewport, unavailable WebGL, no WebGPU adapter, hidden voices, unmodified second-profile concurrency, regional language and headers, winter/summer timezone offsets, custom/IP coordinates, new tabs and simultaneous profile isolation. Firefox's production IP launch path is also tested with an initially unknown IP, pre-navigation restart, cross-origin position readback and restoration of a pre-existing provider URL.

The final regional integration passes for all three installed browsers using their native headless mode (`NODECLOAK_TEST_HEADLESS_BROWSERS=1` is compiled into tests only). Earlier privacy API readback passes with Chrome/Edge windows and native headless Firefox. The production Firefox IP retry test also passes using `MOZ_HEADLESS=1`. Visible frames must return the configured coordinates. Hidden frames may defer native geolocation; the test accepts only the browser's timeout code 3 together with actual hidden visibility, and does not accept real coordinates. The test does not replace visibility or Geolocation APIs in page JavaScript.

WebRTC ICE address exposure, Canvas protection effectiveness and battery API readback have not been separately verified; configuration writes and reversibility are tested. Partitioned persistent session storage is implemented separately; see `partitioned-session-storage-validation.md` for its scope and verification.

## Implementation references

- [Chromium native emulation](https://chromium.googlesource.com/chromium/src/+/main/content/browser/devtools/protocol/emulation_handler.cc)
- [Firefox native geolocation provider](https://github.com/mozilla-firefox/firefox/blob/main/dom/system/NetworkGeolocationProvider.sys.mjs)
- [Firefox fingerprinting protection](https://firefox-source-docs.mozilla.org/toolkit/components/resistfingerprinting/resistfingerprinting/index.html)

Independent review identified one network-error cleanup issue, which was fixed and re-reviewed. The provider fallback and startup retry lifecycle received a separate focused review.
