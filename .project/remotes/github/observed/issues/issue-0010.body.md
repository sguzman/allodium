Implement the first GitHub Discussions vertical slice without making GitHub categories, node IDs, comments, answers, votes, or moderation metadata canonical project authority.

## Current provider boundary

The Allodium GitHub repository currently has Discussions disabled (`has_discussions = false`). The adapter must treat that as an explicit provider capability/bootstrap boundary rather than silently enabling a GitHub feature or inventing category state.

GitHub's Discussions API is GraphQL-first. Creating a Discussion requires a repository node ID, category node ID, title, and body. Close/reopen are separate mutations. Categories are provider-managed repository taxonomy and may be answerable, open-ended, or announcement-shaped. Comments, replies, chosen answers, votes, locks, reactions, and authorship are social/provider state unless a later canonical design explicitly promotes some subset.

## Acceptance criteria

- [ ] Define a forge-neutral canonical discussion root record under `.project/discussions/` with project-owned identity, title, body, and open/closed state.
- [ ] Keep GitHub Discussion number/node ID and URLs in remote mappings/observations only.
- [ ] Do not embed GitHub category node IDs in canonical discussion records.
- [ ] Define an explicit filesystem-owned remote configuration/binding that maps canonical discussion intent to a compatible GitHub Discussion category without making the provider category canonical.
- [ ] Refuse projection when Discussions are disabled instead of silently enabling the provider feature.
- [ ] Refuse creation when no configured/compatible category can be resolved; do not guess the first GitHub category.
- [ ] Make planning deterministic and network-free from canonical state plus persisted mappings/observations/configuration.
- [ ] Persist observed Discussion title, body, state, category identity, node ID, number, URL, and provider revision metadata needed for safe reconciliation.
- [ ] Implement create/update/close/reopen for the canonical root Discussion through the normal inspectable plan/apply path.
- [ ] Use optimistic stale-state refusal before mutable provider writes.
- [ ] Archive provider-side changes to managed root fields before advancing reconstructible observation.
- [ ] Preserve comments, replies, answers, locks, reactions/votes, authorship, and other GitHub-local social state as provider-scoped evidence; do not impersonate or silently promote it into canonical authorship.
- [ ] Do not implement destructive Discussion deletion in v0; canonical absence must not erase provider conversation history.
- [ ] Explicitly document whether category changes are managed projection fields or immutable-after-create for v0 before live mutation is enabled.
- [ ] Add capability-aware tests for `has_discussions = false` and category-resolution failure.
- [ ] If provider bootstrap is completed, dogfood one canonical Discussion through create, provider drift archival/repair, close/reopen semantics, and idempotent re-observation.

## Non-goals for this slice

- Canonical ownership of GitHub Discussion categories themselves.
- Cross-forge reposting of discussion comments/replies.
- Canonical votes, reactions, locks, answer selection, or moderation actions.
- Automatic conversion between Issues and Discussions.
- Silent enablement of GitHub Discussions or creation of provider categories.

This issue is canonical Allodium state. Any GitHub Issue projection of this work item is downstream.

---

**Allodium canonical ID:** `issue-0010`

This GitHub issue is a projection of `.project/issues/issue-0010/`, not the canonical record.