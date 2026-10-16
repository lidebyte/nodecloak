# Sync deletion safety and discoverable recovery

## Confirmed findings

The installed physical profile registry contains the live acsisters profile. A process launched from Codex inherited Windows package AppData redirection and read a different cached registry. GetFinalPathNameByHandle and a nonredirected WMI-created read-only helper confirmed the two physical paths. Absence in the redirected registry did not establish loss of the installed configuration.

The sync decision function previously synthesized a local tombstone when a previously bound local registry entry was absent. If the remote equaled the baseline, it uploaded that tombstone. This is a reproduced deletion propagation risk when the wrong data root, incomplete registry, or interrupted migration leaves an entry absent. The original action that created the user's historical remote tombstone has not been independently established.

Remote tombstones previously also applied automatically to an unchanged live local profile. Startup subsequently rejects a remote-deleted configuration and persists a conflict, but the profile screen had no direct recovery action. Several other launch routes only displayed temporary errors.

## Changes

- Missing local configuration is a conflict rather than inferred deletion. The narrow exception is an identical, already-confirmed baseline/remote tombstone: no config write or local file operation is needed after a completed deletion is purged.
- A live local profile and remote deletion require an explicit choice, even when local fields were unchanged. Explicit local tombstones still synchronize normally.
- Keep-local resolution cannot substitute an old deleted baseline when the actual local configuration is missing.
- Permanent deletion retains the local trash/configuration record until linked cloud deletion is acknowledged, or the user unlinks the profile. This preserves explicit deletion intent and gives an actionable error for pending sync. Unbound and detached profiles remain purgeable.
- The profile screen has a persistent pending-conflicts banner and per-profile recovery actions. Normal launch, save-and-launch, restart, and batch failures refresh cloud status and retain the affected profile identity. Restart confirmation closes so recovery is accessible.
- Overview/site/verification launch conflicts have persistent actionable errors. A toolbar conflict count and sync settings recovery button make pending work discoverable outside the settings page.
- Existing sync modal navigation focuses the requested profile with keyboard-accessible focus. Deletion guidance explains that selecting a deleted state propagates deletion, rather than always claiming keep-local restores data.
- Chinese/English copy and the existing VI tokens/controls are retained. No credentials or login values are added to error text.

## Verification

Rust regressions demonstrated the original missing-entry upload and automatic remote deletion behaviors before the decision changes. Purge regressions demonstrated false conflicts after acknowledged deletion and loss of explicit deletion intent before acknowledgement before their respective repairs.

React regressions demonstrated missing launch recovery, batch refresh, overview persistent action, and modal focus behavior. Coverage also includes create-and-start and restart target retention. Full results: 228 ordinary Rust tests passed (32 opt-in tests ignored), 170 frontend tests passed in 41 files. Production TypeScript/Vite build passed. A synthetic browser preview using the real components verified row recovery opens the sync modal and focuses the conflict. No production cloud actions or user profile deletion/restoration was performed.

The embedded-frontend local desktop repair build completed successfully at `D:\Source\Repos\claudedone\src-tauri\target\debug\nodecloak.exe` (SHA-256 `A5E8C5ABBA38519E6B547DDCF47F25C8A87B35809AC149FA50F3D1E5F16A6174`). The earlier local application was not forcibly closed or replaced; switching to this newly built process remains a runtime acceptance step. No commit, push, or public release was performed.

Development/production credentials and device namespaces remain a separate compatibility-sensitive follow-up. These guards apply regardless of shared accounts or identities. This change does not assert that all possible multi-device conflicts can be eliminated, or that the historical user's conflict has already been resolved.

The subsequent overall test found and repaired a repeated-conflict edge case when a device receives restoration after its deletion was already acknowledged. Updated validation, current build hash and the full native/server/browser results are recorded in [overall sync validation](sync-overall-validation-2026-10-16.md).
