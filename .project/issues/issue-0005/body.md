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

This issue is canonical Allodium state. The canonical review concept must remain forge-neutral even when its first adapter is GitHub.

## Live dogfood evidence — 2026-09-16

`review-0001` was projected to GitHub PR #7 and used as the live review specimen. The canonical filesystem record remained authoritative throughout.

Verified live paths:

- mapped PR observation persists base/head refs plus immutable base/head SHAs;
- canonical body drift was reconciled outward through the normal Actions runtime;
- a GitHub conversation comment, submitted review, inline review comment, and GraphQL review thread were archived as distinct provider-scoped incoming evidence;
- repeated real-API observation produced no duplicate social snapshots;
- changing only canonical `review-0001.state` to `closed` planned `update_pull_request` with `fields = ["state"]` and closed PR #7 through Allodium's Actions runtime;
- the resulting remote closed state was re-observed into ordinary files.

### Provider capability boundary

Automatic `create_pull_request` from the repository's `GITHUB_TOKEN` is currently blocked by GitHub's repository-level Actions policy even though the workflow token has `pull-requests: write`. GitHub returns `403: GitHub Actions is not permitted to create or approve pull requests.` Allodium now diagnoses this as a provider capability/policy boundary and leaves canonical state unmapped rather than pretending creation succeeded.

For the dogfood specimen only, the already-derived `create_pull_request` plan was executed by a separately authorized GitHub executor, after which the reconstructible mapping was recorded and normal Allodium observation/reconciliation took over. This is integration bootstrap evidence, not canonical project authority.

### Remaining M2 gap

The GitHub adapter already receives `merged_at` from pull-request responses, but the observed review schema does not yet persist merge status. M2 remains open until merge observation is represented in ordinary provider-scoped filesystem state. The GitHub Actions create-policy block is external configuration and remains explicitly documented rather than disguised as a successful Actions create.
