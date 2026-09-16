Implement the first canonical milestone projection vertical slice without making GitHub Milestones the owner of milestone identity or lifecycle.

## Acceptance criteria

- [x] Canonical `.project/milestones/milestone-*/milestone.toml` and `description.md` load and validate without GitHub knowledge.
- [x] Milestone IDs are project-owned and must match their directory names; GitHub milestone numbers/URLs remain remote mappings and observations.
- [x] Canonical milestone state is `open` or `closed`; optional `due` dates are validated as ISO 8601 calendar dates.
- [x] Planning is deterministic and network-free.
- [x] GitHub mappings and observed milestone snapshots live under `.project/remotes/github/`.
- [x] Create/update/close reconcile outward through the normal inspectable plan/apply path.
- [x] Applying an update refuses to mutate after live GitHub milestone state differs from the recorded observation.
- [x] Provider-side managed milestone changes are archived as GitHub-scoped incoming evidence before reconstructible observation advances.
- [x] A real canonical `milestone-0001` is projected to GitHub and re-observed idempotently.
- [x] Attaching canonical issues to milestones remains explicitly out of scope until the issue schema gains a canonical milestone reference; provider attachment does not invent that relationship.

The dogfood object for this slice is `.project/milestones/milestone-0001/`. This issue is canonical Allodium state; any GitHub Issue projection is downstream.

## Live dogfood results

Canonical `milestone-0001` projected through the permanent Allodium sync to GitHub Milestone #1. The provider number and URL were persisted only as reconstructible mapping state, and repeated reconciliation converged to a milestone no-op.

The first live create attempt exposed a provider contract edge: GitHub rejected `due_on: null` with HTTP 422 (`nil is not a string`). Allodium now omits `due_on` entirely when canonical `due` is absent, while a present canonical date projects deterministically as end-of-day UTC (`YYYY-MM-DDT23:59:59Z`). The documented GitHub milestone REST contract exposes `due_on` as a timestamp string rather than a nullable field, so v0 refuses to clear an existing provider due date by emitting undocumented JSON null; that limitation remains explicit instead of being guessed around.

A deliberate provider-only title edit changed GitHub Milestone #1 to `PROVIDER DRIFT — M4 milestone dogfood` without touching canonical files. The next permanent reconciliation archived exactly one provider-scoped milestone managed-field event with `fields = ["title"]`, preserved before/after snapshots without asserting an editor identity, planned `update_milestone`, restored the canonical title through a stale-guarded PATCH, and re-observed with zero additional drift. The incoming evidence remains in `.project/remotes/github/incoming/` after repair.

Finally, canonical `milestone-0001.state` changed from `open` to `closed`. Normal CI passed, the projection plan contained exactly `update_milestone` with `fields = ["state"]`, Allodium updated GitHub Milestone #1, re-observed it as closed, validated the filesystem, and persisted the resulting reconstructible observation. No direct provider-side close was used.

Milestone v0 therefore has executable canonical validation, deterministic planning, live create/update/close projection, optimistic stale-write protection, provider-drift preservation and repair, and real idempotence evidence. GitHub remains downstream of the filesystem record.

---

**Allodium canonical ID:** `issue-0008`

This GitHub issue is a projection of `.project/issues/issue-0008/`, not the canonical record.