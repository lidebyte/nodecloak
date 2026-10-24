# macOS browser profile lock compatibility

**Goal:** Avoid permanently blocking an idle Mac browser profile because its singleton lock file remains on disk.

**Architecture:** Keep the existing process check, then distinguish an actively held native file lock from an idle regular file. Check symlink lock host/PID conservatively without following or removing links. Preserve browser-owned cleanup and all profile data.

**Constraints:** Never delete lock files blindly, kill unrelated processes, or treat a lock-check error as unlocked. Windows lock semantics stay unchanged. User's macOS 27 machine is not locally available; record the actual CI versions used.

- [x] Reproduce the persistent regular-file false positive with a failing regression.
- [x] Implement non-destructive native lock checks and conservative host/PID validation.
- [x] Test idle/held/stale/malformed/foreign locks, preservation and release-after-exit.
- [x] Run Windows regression and targeted Apple Silicon/Intel macOS native Edge restart checks.
- [x] Record evidence and report scope and remaining user-machine verification honestly.

## Evidence so far

- The original existence-only check failed the persistent idle-file regression.
  The corrected check passes and leaves lock contents unchanged; held native
  locks remain blocked until released. Symlink host, PID existence and final
  link identity are verified without following or deleting the link.
- Windows: 273 tests passed in serial and four-worker runs. One initial
  unrestricted-parallel run terminated with native status 0xc0000409 without
  a Rust assertion failure; the focused tests and complete controlled-worker
  repeats pass. No Windows production lock behavior changed.
- CI 40113609075, source 0f1a43ed687654ee564b29ee64b62700aad6d66a:
  Apple Silicon macOS 15.7.9 (24G830), Edge 152.0.4191.66,
  Chrome 152.0.7977.83 passed five native lock tests, real document loading
  and cookie persistence over three close/reopen cycles per browser,
  and all 266 applicable Rust tests. Intel exposed an immediate post-close
  lock-release assertion failure; the assertion originally omitted the actual
  error, so transient descriptor inheritance is a supported explanation rather
  than a conclusively captured diagnosis.
- Apple documents that dup/fork references share one flock. The probe now
  explicitly unlocks on every post-acquisition validation path, propagating
  unlock errors. A macOS regression keeps a duplicate descriptor alive and
  checks release on both successful and unsuccessful validation; the live-owner
  fixture also explicitly releases its lock. Browser-owned files remain intact.
  Reference: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/flock.2.html
- Follow-up source 5841d5dd51624713bb547b4fcc05233fc7ad9999: Windows focused
  lock regression 4/4 and complete four-worker Rust regression 273 passed,
  37 ignored (59.50 seconds). actionlint and diff checks passed; reviewer found
  no important blocker. Follow-up macOS run 40114846430: Apple Silicon
  passed all six native lock regressions, three real close/reopen cycles
  per browser including persistent cookies, and all 267 applicable Rust tests.
  Intel also passed all six native lock regressions, all six real browser
  close/reopen cycles with cookie persistence, and all 267 applicable Rust tests.
  Both runners used macOS 15.7.9 (24G830). Intel used Edge 151.0.4129.101 and
  Chrome 151.0.7922.174; Apple Silicon used Edge 152.0.4191.66 and Chrome
  152.0.7977.83. The complete run finished successfully on both architectures:
  https://github.com/nodecloak/nodecloak/actions/runs/40114846430
- The previous CI run exposed macOS /var temporary-path aliases in the new
  diagnostics tests. Only disposable fixture paths were canonicalized;
  production path guards remain unchanged. The native browser fixture waits
  for a real page report and a persistent cookie before closing each cycle.
- No macOS 27 user-machine result or new published installer is claimed.
  Private logs and regression evidence are under .qa/macos-lock-20261025.
