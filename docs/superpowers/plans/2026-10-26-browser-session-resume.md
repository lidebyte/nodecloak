# Resume browser profiles from local saved session state

Ordinary profile Start restores previously open pages instead of always replacing them with the configured start page. Explicit site/diagnostic and MCP launches keep their requested destinations. No public release has been made for this change.

Implemented behavior:

- Controlled manual Windows Chrome reopens the browser's last closed window after its settings controller is ready. It retains native history, recoverable form state and scroll. Cold restore was rejected because first website scripts could run before profile protection.
- Controlled manual Edge and macOS Chrome restore its recorded normal-profile tab URLs through the protected navigation path. It uses ordinary browser cache policy; it does not currently restore form/scroll state. Native Edge reopen was rejected by the first-script regression, so that unsafe path is not shipped.
- Edge captures only the initial normal browser context and preserves that boundary across controller reconnection. Incognito/ambiguous contexts, prerenders and Workers are excluded. A live window containing only internal tabs clears old website URLs; browser shutdown keeps the last nonempty window snapshot.
- Uncontrolled Chromium launches can use native startup restore. MCP never invokes the new restore path or captures manual tab state.
- Firefox parks native session files before temporary extension/configuration startup, including upgrade backups. Native readback tries clean/recovery/previous/upgrade files in order, using Firefox decompression and optional decryption and checking its format version.
- After the startup gate confirms the existing companion and non-WebDriver state, Firefox restores session cookies first, restores the saved active tab into the owned gate with SessionStore.setTabState, and appends other saved tabs/windows without replacing newly opened user content.
- Firefox uses temporary native RDP configuration, never WebDriver for manual session restoration. System-region profiles only wait for window readiness and retain their original locale; native region overrides are applied only when requested.
- Firefox native save promises are not treated as durable success. Readback checks tab coverage and unchanged native bytes across decoding, then binds a receipt to SHA256. Redirects or save/readback changes keep the working browser open and retain original snapshots.
- One prior original snapshot is kept in nodecloak-session-backup. Previously unconfirmed snapshots move to uniquely named recovery archives instead of being deleted by later blank sessions. Those recovery archives are intentionally retained for data safety.
- Interrupted setup rolls back original native files; newly generated files are retained separately. No real user profile data or native-host registration was changed by verification. The signed-companion fixture reused the installed host read-only and installed the companion only inside a disposable profile.

Limits:

- Browser/site rules control cache freshness and which fields/session cookies survive. A closed process has no frozen JavaScript heap; this is not a promise of zero network requests or universal login persistence.
- Controlled Windows Chrome reopens the last native window. Edge and macOS Chrome restore pages into a window, with no native form/scroll snapshot. Firefox restores its saved windows.
- The existing Firefox companion targets document/frame readback; Worker fingerprint marker behavior remains its existing baseline. Native Worker timezone is preserved.
- Native integration checks ran on Windows. macOS/Linux use the same guarded platform code but were not tested on physical devices here.

Verification:

- [x] Windows Chrome and Edge first-script fingerprint/timezone/WebDriver checks on restart.
- [x] Chrome native saved draft and 700px scroll restoration.
- [x] Edge normal-context-only capture, live-empty vs shutdown, and ordinary periodic capture.
- [x] Chrome/Edge MCP launches do not replay the prior manual URLs.
- [x] Firefox plain and signed-companion restart: saved draft, 700px scroll, session-only cookie, native timezone, WebDriver false; document/frame fingerprint follows existing settings.
- [x] Unit rollback, receipt binding, redirect/save divergence, native backup fallback, upgrade-only fencing and two-launch unconfirmed-original retention.
- [x] Backend: 299 passed; native browser opt-in checks separately passed.
- [x] Frontend: 212 passed; production build passed; existing sync workspace changes preserved.
- [x] Firefox script/companion checks: 10 passed.
- [x] Final system-region Firefox check preserved en-US Intl locale, fr-FR HTTP language, Asia/Singapore timezone, saved draft/scroll and session cookie; refreshed desktop binary built.

Release review identified a slow-start Edge capture race: an initial blank helper could erase the prior URL snapshot before Resume arrived. Capture now remains disabled until a successful restoration or requested navigation, and retains that state across reconnect. The native regression explicitly delays Resume by three seconds and confirmed failure before the fix. The release pipeline runs the protected Chromium and both plain Firefox restart checks on all three build platforms.

Cross-platform release checks found the native Reopen Closed Window accelerator unreliable on macOS Chrome. Controlled macOS Chrome therefore uses the same protected normal-context tab URL recovery as Edge; native form/scroll state is not promised on this path. The shared capture predicate is reused at launch, capture and restore. The native check asserts protected page recovery on both Mac Chromium browsers.

Final failure-path review found that an unreachable saved URL could be mistaken for a controller failure and close the profile. URL recovery now acknowledges a bounded local queue, restores one protected page at a time, keeps navigation-error tabs, and continues later URLs. Capture remains fenced until the queue completes, including across reconnection. Control errors stop uncertain replay and retain the original snapshot. A refused-first/good-second native regression reproduced the old failure and passed after the fix; logging persists only a network error code.

The macOS default-region native recovery check exposed a same-actor RDP reply race: root window/process notifications were accepted as command replies, shifting subsequent responses. Native calls now require method-specific reply payloads and leave typed notifications out of request completion. A framed-socket regression interleaves root/console notifications before replies and verifies the complete evaluate ACK/result handoff; it failed on the old code and passed after the fix. Mozilla protocol sources: https://raw.githubusercontent.com/mozilla-firefox/firefox/main/devtools/shared/specs/root.js and https://raw.githubusercontent.com/mozilla-firefox/firefox/main/devtools/shared/specs/webconsole.js .
