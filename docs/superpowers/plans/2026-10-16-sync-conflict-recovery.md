# Sync deletion protection and recovery entry points

**Goal:** Prevent an absent local registry entry from deleting a synchronized cloud profile and make every launch-blocking sync conflict directly actionable.

**Architecture:** Keep explicit profile tombstones distinct from missing configuration. Preserve live local profiles when a remote tombstone arrives until the user makes a choice. Reuse the existing sync modal, with profile-targeted navigation from the profile list, rather than adding a competing conflict resolver.

**Tech Stack:** Rust sync decision tests; React, existing VI controls, Vitest DOM tests.

**Evidence:** Codex package AppData redirection produced a different local registry while sharing the production endpoint. `decide` currently manufactures a deletion document from a missing registry entry. This is a reproducible deletion propagation path, not proof of the historical writer/action that created this user's tombstone. The physical registry still has acsisters. Startup saves a conflict but the profile screen has no recovery action.

## Constraints

- No live profile/configuration/cloud conflict mutations during implementation or tests.
- Preserve prior Firefox storage fixes and unrelated user changes.
- Do not infer deletion from filesystem or registry absence.
- Explicit local deletion still synchronizes; explicit conflict choices remain required for restoring/accepting remote deletion.
- Match the existing brand controls, support keyboard focus and Chinese/English copy.
- No release, push, installed binary replacement, or forced application exit in this change.

## Tasks

- [x] Write and run failing Rust regressions for missing local records, live local vs remote tombstones and explicit local tombstones. Review the missing-local resolution guard.
- [x] Implement conservative decisions and reject “keep local” when no local configuration exists, rather than uploading a manufactured tombstone. Add purge acknowledgement/intention regressions and guards.
- [x] Write and run failing React tests for launch error → targeted sync modal and pending conflicts visible on the profile screen/settings.
- [x] Add profile conflict actions, focused modal navigation, helpful field descriptions, and an immediate status refresh after failed launch. Cover row/save/restart/batch and overview/site/verification entry points.
- [x] Verify targeted tests, full ordinary Rust/frontend suites, production TypeScript build and diff checks. See `docs/specs/sync-conflict-recovery-2026-10-16.md` for results and limits. Read-only independent review found no remaining significant issues after fixes.

Development/production namespace separation remains a distinct compatibility-sensitive follow-up. This change must protect deletion semantics even when two installations or redirected directories share an account or device identity.
