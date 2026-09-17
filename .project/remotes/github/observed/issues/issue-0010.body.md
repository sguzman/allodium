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
- [x] Persist observed Discussion title, body, state, category identity, node ID, number, URL, and provider revision metadata needed for safe reconciliation.
- [x] Implement create/update/close/reopen for the canonical root Discussion through the normal inspectable plan/apply path.
- [x] Use optimistic stale-state refusal before mutable provider writes.
- [x] Archive provider-side changes to managed root fields before advancing reconstructible observation.
- [x] Preserve comments, replies, answers, locks, reactions/votes, authorship, and other GitHub-local social state as provider-scoped evidence; do not impersonate or silently promote it into canonical authorship.
- [x] Do not implement destructive Discussion deletion in v0; canonical absence must not erase provider conversation history.
- [x] Explicitly document whether category changes are managed projection fields or immutable-after-create for v0 before live mutation is enabled.
- [x] Add capability-aware tests for `has_discussions = false` and category-resolution failure.
- [ ] If provider bootstrap is completed, dogfood one canonical Discussion through create, provider drift archival/repair, close/reopen semantics, and idempotent re-observation.

## Implemented boundary

Canonical `discussion-0001` is ordinary filesystem state, while its GitHub category selector is remote configuration under `.project/remotes/github/discussions.toml`. The offline planner freezes the resolved provider category node ID into the remote mapping on first creation and refuses later configuration changes that would silently reclassify a mapped conversation. Provider category drift is observable provider taxonomy rather than canonical Discussion meaning.

The repository capability boundary has been exercised live. Permanent reconciliation observed `has_discussions = false`, persisted `allodium.github.observed-discussion-capabilities/v0` with the repository node ID and an empty category inventory, planned `discussions_bootstrap_required`, applied it as a non-mutating provider requirement, re-observed successfully, and performed no Discussion creation. Allodium therefore does not silently enable GitHub Discussions.

The full root runtime is now implemented and gated offline behind that live capability boundary. It supports `createDiscussion`, managed title/body updates, `closeDiscussion`, `reopenDiscussion`, mapped observation, and reconstructible snapshots. Before mutable writes it re-queries the mapped Discussion and refuses stale plans when provider revision or managed root state changed. Managed provider changes to title, body, state, or category are archived with before/after snapshots before observation advances. Category drift is archived but deliberately not repaired automatically. There is no destructive Discussion deletion path.

Provider social-state archival is also implemented as an isolated GitHub evidence stream. `allodium.github.discussion-social/v0` snapshots preserve root and comment authorship, editors, author associations, comments and replies, selected answers, locks/state reasons, upvote counts, reactions, polls and vote totals, minimization/deletion metadata, timestamps, and provider identities without making any of them canonical Allodium authorship or cross-forge state. Snapshot ordering is normalized and unchanged re-observation is idempotent; changed snapshots append incoming provenance before the reconstructible current snapshot advances.

The social-ingress gate passed rustfmt, the complete locked workspace suite, repository validation, idempotence and ordering-normalization tests, and the same real-repository safety smoke requiring `discussions_bootstrap_required` while forbidding `create_discussion`. The implementation is therefore ready behind the capability boundary, but live social-state dogfood remains impossible while GitHub Discussions are disabled.

The permanent GitHub sync workflow is intended to react to both Discussion-root and Discussion-comment events once the provider feature is available. GitHub-local activity still flows through normal observe/archive/reconcile behavior rather than becoming canonical state from webhook payloads.

Live create/update/close/reopen and adversarial provider-drift dogfood remains blocked only by the provider feature being disabled. `issue-0010` stays open until that provider acceptance proof can be performed.

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