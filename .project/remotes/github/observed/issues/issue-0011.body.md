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

- [x] Define an ordinary-file `allodium.board/v0` canonical root under `.project/boards/` with project-owned identity, title, description, and explicit item membership.
- [x] Make board items refer to canonical Allodium object IDs instead of copying provider IDs or making GitHub Project items authoritative.
- [x] Define typed canonical board fields separately from item values; reject values that do not conform to their field type.
- [x] Define views separately from board data so table/board/roadmap presentation does not redefine item state.
- [x] Model board grouping as a view operation over a field; do not introduce canonical column objects merely because one provider renders columns.
- [x] Validate duplicate IDs, dangling item references, unknown field references, invalid option references, and incompatible field values.
- [ ] Keep GitHub Project number/node ID, item IDs, field IDs, option/iteration IDs, view IDs, URLs, and owner identity under `.project/remotes/github/` only.
- [x] Require explicit provider configuration for the GitHub Project owner authority (`user` or `organization`) and projection target; do not infer cross-authority identity from matching names.
- [x] Establish a separate Projects credential/capability boundary. Repository `GITHUB_TOKEN` must not be treated as sufficient Projects authority.
- [x] When Projects credentials are absent, surface a non-mutating `projects_auth_required`/capability operation without blocking Issues, Reviews, Wiki, Releases, Milestones, Labels, or Discussions reconciliation.
- [x] Keep Projects credentials environment/secret-only; never serialize tokens or derived secrets into `.project/`.
- [x] Make GitHub Projects planning deterministic and network-free from canonical state plus persisted provider configuration/mappings/observations.
- [x] Initially project only canonical object families with a defined identity mapping; do not invent canonical state for provider DraftIssue items.
- [ ] Preserve provider-created items, field edits, and other foreign Project activity as GitHub-scoped evidence before deciding whether any subset should be promoted.
- [ ] Use stable provider identities for mappings and optimistic observation checks before mutable writes.
- [x] Do not implement destructive Project/item deletion until the canonical absence semantics are explicit.
- [x] Add fixtures/tests that prove the canonical board model is independent of GitHub ProjectV2 IDs and provider view vocabulary.
- [ ] If a separately authorized Projects credential is available, dogfood one Allodium board through provider creation/mapping, item projection, field/value reconciliation, provider drift archival, and idempotent re-observation.

## Canonical core progress

The canonical-only increment is now executable and dogfooded. `allodium.board/v0` is represented by ordinary files under `.project/boards/<board-id>/`, with independent `fields/`, `items/`, and `views/` directories. The checked-in `board-0001` tracks canonical `issue-0010` and `issue-0011`; its development board view groups the project-owned `status` field, while its roadmap view references the project-owned `target-date` field. There is no canonical column object and no GitHub ProjectV2 identifier anywhere in this representation.

The validator now enforces board/directory identity, field and view filename identity, supported field/value types, stable single-select option IDs, canonical issue/review membership, no dangling or duplicate item membership, known field/value references, valid select options, strict calendar dates, and layout-specific view rules. Checked-in valid and invalid board fixtures and module-level tests passed the full locked workspace gate. `docs/boards-v0.md` documents the format and provider boundary.

## Projects capability progress

The first GitHub Projects adapter layer is now executable. `.project/remotes/github/projects.toml` explicitly configures the GitHub authority kind/name, the environment-variable name from which separate Projects credentials may be supplied, and which canonical boards are enabled for projection. The credential value itself is never serialized. The offline planner reads only canonical state plus persisted provider configuration/capability observations; it does not inspect the environment or make network calls.

Projects authorization is intentionally independent from the repository adapter token. The runtime capability observer checks only the separately named Projects credential. If no credential is present it writes a non-secret capability snapshot with `credential_available = false` and `authenticated = false`. If a credential is present, it must successfully probe the explicitly configured GitHub user/organization authority through GraphQL before `authenticated = true` and the owner node ID may be persisted. Rejected credentials do not become authority, and tests prove successful observation never writes token contents.

The live Allodium sync dogfooded the missing-credential path after the capability bridge landed. It observed 11 mapped issues, one review, Wiki, Release, milestone, label, Discussion capability state, and one Projects capability snapshot in the same run. The resulting plan contained only the already-known `wiki_bootstrap_required`, `discussions_bootstrap_required`, and `projects_auth_required` operations. Apply performed zero Projects mutations and did not block or mutate the other surfaces. Re-observation produced no new drift and validation passed.

## Projects identity substrate progress

The provider identity substrate landed as `74d8e44f251977585ceccbd87ef1393384d7ae6a`. Board bindings now require an explicit target policy: `managed` means Allodium may eventually create its own ProjectV2; `existing` requires a positive explicit Project number. A matching title is never an identity rule. The provider namespace now has typed stable mappings for Project number/node ID/URL/owner identity, Project item node IDs plus content node IDs, field node IDs and data types, single-select option IDs, and view node IDs. Observed Project identity and issue/pull-request GraphQL content identity also have explicit provider-scoped record types.

The deterministic planner now distinguishes managed creation, binding an explicitly numbered existing Project, missing Project observation, Project/owner identity disagreement, missing repository mapping, missing GraphQL content identity, missing item membership identity, missing field/option/view identity, and canonical Project metadata drift. Even with every known identity satisfied it still emits only `projects_runtime_required`; ProjectV2 mutation remains behind a later runtime gate. The planner never emits Project or item deletion.

The first identity gate intentionally failed before publication because two pre-existing capability-runtime test fixtures still generated the older `projects.toml` shape without the newly mandatory target. All 79 core tests had already passed, which isolated the defect to fixture/schema skew. The fix kept `target` mandatory rather than introducing an implicit compatibility default, updated those fixtures to opt into `target = "managed"`, and reran the self-cleaning gate. The successful run passed 79 core tests, 8 checked-in fixture tests, the ingress idempotence test, 39 GitHub adapter tests, formatting, and repository validation, then repeated the complete suite after rebase before publishing the clean product commit.

Permanent sync run 188 subsequently checked out exactly `74d8e44f251977585ceccbd87ef1393384d7ae6a`, observed and re-observed every existing GitHub surface, retained the expected `wiki_bootstrap_required`, `discussions_bootstrap_required`, and `projects_auth_required` plan, performed zero Projects mutations, validated the repository, and ended with `No durable GitHub state change.` This proves the richer identity substrate composes cleanly with the no-Projects-credential degraded mode.

The next Projects increment is read-only ProjectV2 observation and provider-state archival. It must populate and verify stable Project/item/field/option/view/content identities when separately authorized, preserve foreign Project and DraftIssue activity as provider evidence, and establish the observation basis for later optimistic mutation guards without promoting provider state into canonical state.

## Design constraints

This slice should resist two opposite failures. It must not reduce Allodium to a lowest-common-denominator fake forge API, but it also must not make GitHub's flexible ProjectV2 schema the ontology of the project itself. Provider richness can live in the GitHub adapter and remote namespace while the canonical board remains directly understandable and editable with ordinary filesystem tools.

The implementation sequence is canonical board core, then provider capability/configuration, then provider identity/mapping and observation, and only then ProjectV2 mutation. Each layer must remain useful and valid when the next provider capability is unavailable.

This issue is canonical Allodium state. Any GitHub Issue projection of this work item is downstream.

---

**Allodium canonical ID:** `issue-0011`

This GitHub issue is a projection of `.project/issues/issue-0011/`, not the canonical record.