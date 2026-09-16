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

---

**Allodium canonical ID:** `issue-0005`

This GitHub issue is a projection of `.project/issues/issue-0005/`, not the canonical record.