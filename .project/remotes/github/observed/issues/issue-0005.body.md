Implement the first review projection vertical slice without making GitHub Pull Requests the canonical model.

## Acceptance criteria

- Canonical `.project/reviews/<id>/` records load and validate without GitHub knowledge.
- Stable canonical-review ↔ GitHub-PR mappings live under the GitHub remote namespace.
- GitHub PR observations are reconstructible ordinary files and include enough revision identity to guard outbound mutations.
- `plan` can express create/update/close review projection operations without network access.
- `apply` can create, update, and close a GitHub PR from an explicit plan.
- PR conversation comments, submitted reviews, and inline review threads are archived as provider-scoped incoming evidence rather than canonical prose.
- Merge state is observed without pretending GitHub's merge operation is canonical identity.
- At least one real canonical Allodium review is dogfooded into a GitHub PR and re-observed idempotently.
- The deliberate dogfood PR is integration evidence, not a return to PR-centric development workflow.

This issue is canonical Allodium state. The canonical review concept remains forge-neutral even when its first adapter is GitHub.

## Completion evidence — 2026-09-16

`review-0001` was projected to GitHub PR #7 and used as the live review specimen. The canonical filesystem record remained authoritative throughout.

Verified live behavior:

- mapped PR observation persists base/head refs plus immutable base/head SHAs;
- canonical body drift was reconciled outward through the normal Actions runtime;
- a GitHub conversation comment, submitted review, inline review comment, and GraphQL review thread were archived as distinct provider-scoped incoming evidence;
- repeated real-API observation produced no duplicate social snapshots;
- changing only canonical `review-0001.state` to `closed` planned `update_pull_request` with `fields = ["state"]` and closed PR #7 through Allodium's Actions runtime;
- the resulting remote closed state was re-observed into ordinary files;
- deliberately reopening PR #7 externally archived a provider-side managed-field change, refreshed the remote revision/base SHA, and caused Allodium to reassert canonical `closed` state safely rather than accepting provider state as authority;
- the externally observed reopen survives as append-only provider evidence even after canonical reconciliation;
- GitHub merge state is derived from provider `merged_at`: a non-null value produces observed `state = "merged"`, which the planner treats as terminal remote evidence rather than an implicit canonical merge command;
- observed review state now also preserves optional `merged_at` for provider provenance while remaining backward-compatible with earlier snapshots that lack the field.

## Provider capability boundary

Automatic `create_pull_request` from this repository's `GITHUB_TOKEN` is currently blocked by GitHub's repository-level Actions policy even though the workflow token has `pull-requests: write`. GitHub returns `403: GitHub Actions is not permitted to create or approve pull requests.` Allodium diagnoses this explicitly as a provider capability/policy boundary and leaves canonical state unmapped rather than pretending creation succeeded.

The create planner and adapter path are implemented and tested, and the live request reached GitHub's pull-request creation endpoint before being rejected by that repository policy. For the dogfood specimen only, the already-derived `create_pull_request` plan was executed by a separately authorized GitHub executor; the reconstructible mapping was then recorded and normal Allodium observation/reconciliation took over.

This repository setting is an operational prerequisite for unattended PR creation with `GITHUB_TOKEN`, not canonical project state and not a reason to grant GitHub authority over the review model.

## Result

The M2 GitHub review vertical slice is complete. GitHub PRs are projections and interaction surfaces around forge-neutral canonical reviews. Provider mutations are observed, preserved with provenance, stale-checked, and reconciled without silently becoming canonical state.

---

**Allodium canonical ID:** `issue-0005`

This GitHub issue is a projection of `.project/issues/issue-0005/`, not the canonical record.