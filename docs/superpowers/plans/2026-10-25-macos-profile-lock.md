# macOS browser profile lock compatibility

**Goal:** Avoid permanently blocking an idle Mac browser profile because its singleton lock file remains on disk.

**Architecture:** Keep the existing process check, then distinguish an actively held native file lock from an idle regular file. Check symlink lock host/PID conservatively without following or removing links. Preserve browser-owned cleanup and all profile data.

**Constraints:** Never delete lock files blindly, kill unrelated processes, or treat a lock-check error as unlocked. Windows lock semantics stay unchanged. User's macOS 27 machine is not locally available; record the actual CI versions used.

- [ ] Reproduce the persistent regular-file false positive with a failing regression.
- [ ] Implement read-only native lock checks and conservative host/PID validation.
- [ ] Test idle/held/stale/malformed/foreign locks, preservation and release-after-exit.
- [ ] Run Windows regression and targeted Apple Silicon/Intel macOS native Edge restart checks.
- [ ] Record evidence and report scope and remaining user-machine verification honestly.
