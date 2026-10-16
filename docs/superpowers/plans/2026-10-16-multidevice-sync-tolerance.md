# Multi-device synchronization tolerance

> Execute the checked steps sequentially, using failing regressions before implementation.

**Goal:** Automatically settle ordinary device handoff and optimistic write races, while preserving explicit choices for incompatible edits and deletions.

**Architecture:** Keep encrypted object revisions and lease fencing. Retry configuration reconciliation against fresh cloud data; release only the current device/generation after closed-profile synchronization. Classify transient conditions as waiting in the status projection without changing the persisted schema.

**Tech stack:** Rust/Tauri, Go/PostgreSQL, React, existing isolated native-client harness.

## Constraints

- Preserve both devices' website sessions; never steal a live lease automatically.
- No automatic acceptance of remote deletion or replacement of divergent login snapshots.
- MCP preview version guards and cancellation continue to fence all retries.
- No production-account mutations during tests. The optimization request initially covered local validation. Follow-up publication is tracked by the 3.1.4 release plan.

## Steps

- [x] Add failing Go regressions for generation-scoped lease release, foreign release and stale generation release; run against disposable PostgreSQL.
- [x] Implement DELETE /v1/objects/{id}/lease, expiring only the authenticated owner's exact generation. Return JSON without deleting the fence row.
- [x] Add failing native tests for closed-device handoff, startup configuration catch-up and concurrent configuration revision retry. Preserve existing true-conflict and deletion tests.
- [x] Extract per-profile configuration reconciliation. Fetch fresh state and retry revision conflicts up to three times; apply merged local configuration only after confirmed upload. Exhaustion is queued for the next automatic cycle.
- [x] Reconcile configuration automatically before session preparation at launch. Release a closed profile's own lease after successful synchronization, including unchanged sessions; retain generation on old-server/transport failures for a later retry.
- [x] Add status and frontend tests that distinguish waiting from actionable conflicts; render one concise waiting section using existing VI components and Chinese/English text.
- [x] Run native/server two-device scenarios, full unit suites, frontend build, review and record exact results and limits.

Validation: [multi-device synchronization tolerance](../../specs/multi-device-sync-tolerance-2026-10-16.md). Fresh-launch acquisition advances the fence while renewals retain it; session deletion also releases the matching lease.
