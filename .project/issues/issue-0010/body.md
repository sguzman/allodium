Implement the first GitHub Discussions vertical slice without making GitHub categories, node IDs, comments, answers, votes, or moderation metadata canonical project authority.

## Current provider boundary

The Allodium GitHub repository currently has Discussions disabled (`has_discussions = false`). The adapter must treat that as an explicit provider capability/bootstrap boundary rather than silently enabling a GitHub feature or inventing category state.

GitHub's Discussions API is GraphQL-first. Creating a Discussion requires a repository node ID, category node ID, title, and body. Close/reopen are separate mutations. Categories are provider-managed repository taxonomy and may be answerable, open-ended, or announcement-shaped. Comments, replies, chosen answers, votes, locks, reactions, and authorship are social/provider state unless a later canonical design explicitly promotes some subset.

## Acceptance criteria

- [x] Define a forge-neutral canonical discussion root record under `.project/discussions/` with project-owned identity, title, body, and open/closed state.
- [x] Keep GitHub Discussion number/node ID and URLs in remote mappings/observations only.
- [x] Do not embed GitHub category node IDs in canonical discussion records.
- [x] Define an explicit filesystem-owned remote configuration/binding that maps canonical discussion intent to a compatible GitHub Discussion category without making the provider category canonical.
- [x] Refuse projection when Discussions are disabled instead of silently enabling the provider feature.
- [x] Refuse creation when no configured/compatible category can be resolved; do not guess the first GitHub category.
- [x] Make planning deterministic and network-free from canonical state plus persisted mappings/observations/configuration.
- [ ] Persist observed Discussion title, body, state, category identity, node ID, number, URL, and provider revision metadata needed for safe reconciliation.
- [ ] Implement create/update/close/reopen for the canonical root Discussion through the normal inspectable plan/apply path.
- [ ] Use optimistic stale-state refusal before mutable provider writes.
- [ ] Archive provider-side changes to managed root fields before advancing reconstructible observation.
- [ ] Preserve comments, replies, answers, locks, reactions/votes, authorship, and other GitHub-local social state as provider-scoped evidence; do not impersonate or silently promote it into canonical authorship.
- [x] Do not implement destructive Discussion deletion in v0; canonical absence must not erase provider conversation history.
- [x] Explicitly document whether category changes are managed projection fields or immutable-after-create for v0 before live mutation is enabled.
- [x] Add capability-aware tests for `has_discussions = false` and category-resolution failure.
- [ ] If provider bootstrap is completed, dogfood one canonical Discussion through create, provider drift archival/repair, close/reopen semantics, and idempotent re-observation.

## Implemented boundary so far

Canonical `discussion-0001` is ordinary filesystem state, while its GitHub category selector is remote configuration under `.project/remotes/github/discussions.toml`. The offline planner freezes the resolved provider category node ID into the remote mapping on first creation and refuses later configuration changes that would silently reclassify a mapped conversation. Provider category drift is observable provider taxonomy rather than canonical Discussion meaning.

The safe runtime bridge is now landed: normal GitHub observation can persist repository Discussion capability/category snapshots, the combined planner can emit `observe_discussion_capabilities`, and apply understands both capability observation and the non-mutating `discussions_bootstrap_required` operation. No Discussion create/update GraphQL mutation is enabled yet. The next live acceptance is that the current repository (`has_discussions = false`) converges to the explicit bootstrap requirement without attempting creation.

## Non-goals for this slice

- Canonical ownership of GitHub Discussion categories themselves.
- Cross-forge reposting of discussion comments/replies.
- Canonical votes, reactions, locks, answer selection, or moderation actions.
- Automatic conversion between Issues and Discussions.
- Silent enablement of GitHub Discussions or creation of provider categories.

This issue is canonical Allodium state. Any GitHub Issue projection of this work item is downstream.
