Define Allodium's first project-owned board model before implementing GitHub Projects projection. The canonical model must describe project work in filesystem terms rather than cloning GitHub's current ProjectV2 ontology.

## Provider audit

GitHub's current Projects surface is not fundamentally a fixed-column kanban board. A Project is a collection of items plus configurable fields and one or more views. Table, board, and roadmap are presentation layouts over the same underlying item/field data; in a board view, columns are produced by grouping on a field rather than by a separate durable column object.

GitHub Projects are also scoped to a user or organization rather than to one repository. Provider identities therefore include project, item, field, option, iteration, and view node IDs plus an owning GitHub authority. None of those belong in canonical Allodium state.

The authentication boundary is materially different from the repository surfaces already implemented. GitHub documents that the repository-scoped Actions `GITHUB_TOKEN` cannot access Projects. Project access therefore requires a separately authorized GitHub App or personal access token. Allodium must model that as an optional provider capability and must never persist the credential or silently widen the ordinary repository-sync authority.

## Canonical ontology

The v0 direction is:

```text
board
├── items      references to canonical Allodium objects
├── fields     typed project-owned metadata definitions
├── values     item values for those canonical fields
└── views      presentation/grouping/filtering/sorting definitions
```

A kanban column is therefore not a first-class canonical object. It is a board-view grouping over a field value. A roadmap is likewise a view over date/iteration-like data rather than a different project database.

Canonical board membership should initially reference object families Allodium already owns, beginning with issues and reviews. GitHub DraftIssue items are provider objects and must not be silently promoted into canonical issues merely because GitHub Projects can contain them.

## Acceptance criteria

- [ ] Define an ordinary-file `allodium.board/v0` canonical root under `.project/boards/` with project-owned identity, title, description, and explicit item membership.
- [ ] Make board items refer to canonical Allodium object IDs instead of copying provider IDs or making GitHub Project items authoritative.
- [ ] Define typed canonical board fields separately from item values; reject values that do not conform to their field type.
- [ ] Define views separately from board data so table/board/roadmap presentation does not redefine item state.
- [ ] Model board grouping as a view operation over a field; do not introduce canonical column objects merely because one provider renders columns.
- [ ] Validate duplicate IDs, dangling item references, unknown field references, invalid option references, and incompatible field values.
- [ ] Keep GitHub Project number/node ID, item IDs, field IDs, option/iteration IDs, view IDs, URLs, and owner identity under `.project/remotes/github/` only.
- [ ] Require explicit provider configuration for the GitHub Project owner authority (`user` or `organization`) and projection target; do not infer cross-authority identity from matching names.
- [ ] Establish a separate Projects credential/capability boundary. Repository `GITHUB_TOKEN` must not be treated as sufficient Projects authority.
- [ ] When Projects credentials are absent, surface a non-mutating `projects_auth_required`/capability operation without blocking Issues, Reviews, Wiki, Releases, Milestones, Labels, or Discussions reconciliation.
- [ ] Keep Projects credentials environment/secret-only; never serialize tokens or derived secrets into `.project/`.
- [ ] Make GitHub Projects planning deterministic and network-free from canonical state plus persisted provider configuration/mappings/observations.
- [ ] Initially project only canonical object families with a defined identity mapping; do not invent canonical state for provider DraftIssue items.
- [ ] Preserve provider-created items, field edits, and other foreign Project activity as GitHub-scoped evidence before deciding whether any subset should be promoted.
- [ ] Use stable provider identities for mappings and optimistic observation checks before mutable writes.
- [ ] Do not implement destructive Project/item deletion until the canonical absence semantics are explicit.
- [ ] Add fixtures/tests that prove the canonical board model is independent of GitHub ProjectV2 IDs and provider view vocabulary.
- [ ] If a separately authorized Projects credential is available, dogfood one Allodium board through provider creation/mapping, item projection, field/value reconciliation, provider drift archival, and idempotent re-observation.

## Design constraints

This slice should resist two opposite failures. It must not reduce Allodium to a lowest-common-denominator fake forge API, but it also must not make GitHub's flexible ProjectV2 schema the ontology of the project itself. Provider richness can live in the GitHub adapter and remote namespace while the canonical board remains directly understandable and editable with ordinary filesystem tools.

The first implementation increment should therefore be canonical-only: board/field/value/view records plus validation and fixtures. GitHub mapping and runtime follow only after that format is executable.

This issue is canonical Allodium state. Any GitHub Issue projection of this work item is downstream.

---

**Allodium canonical ID:** `issue-0011`

This GitHub issue is a projection of `.project/issues/issue-0011/`, not the canonical record.