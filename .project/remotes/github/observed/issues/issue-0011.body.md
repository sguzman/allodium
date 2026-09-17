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
- [x] Keep GitHub Project number/node ID, item IDs, field IDs, option/iteration IDs, view IDs, URLs, and owner identity under `.project/remotes/github/` only.
- [x] Require explicit provider configuration for the GitHub Project owner authority (`user` or `organization`) and projection target; do not infer cross-authority identity from matching names.
- [x] Establish a separate Projects credential/capability boundary. Repository `GITHUB_TOKEN` must not be treated as sufficient Projects authority.
- [x] When Projects credentials are absent, surface a non-mutating `projects_auth_required`/capability operation without blocking Issues, Reviews, Wiki, Releases, Milestones, Labels, or Discussions reconciliation.
- [x] Keep Projects credentials environment/secret-only; never serialize tokens or derived secrets into `.project/`.
- [x] Make GitHub Projects planning deterministic and network-free from canonical state plus persisted provider configuration/mappings/observations.
- [x] Initially project only canonical object families with a defined identity mapping; do not invent canonical state for provider DraftIssue items.
- [x] Preserve provider-created items, field edits, and other foreign Project activity as GitHub-scoped evidence before deciding whether any subset should be promoted.
- [x] Use stable provider identities for mappings and optimistic observation checks before mutable writes.
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

At this stage the deterministic planner distinguished managed creation, binding an explicitly numbered existing Project, missing Project observation, Project/owner identity disagreement, missing repository mapping, missing GraphQL content identity, missing item membership identity, missing field/option/view identity, and canonical Project metadata drift while deliberately stopping at a non-mutating runtime boundary. That staged boundary was later replaced by the gated non-destructive mutation runtime described below. Project/item deletion has never been inferred from canonical absence.

The first identity gate intentionally failed before publication because two pre-existing capability-runtime test fixtures still generated the older `projects.toml` shape without the newly mandatory target. All 79 core tests had already passed, which isolated the defect to fixture/schema skew. The fix kept `target` mandatory rather than introducing an implicit compatibility default, updated those fixtures to opt into `target = "managed"`, and reran the self-cleaning gate. The successful run passed 79 core tests, 8 checked-in fixture tests, the ingress idempotence test, 39 GitHub adapter tests, formatting, and repository validation, then repeated the complete suite after rebase before publishing the clean product commit.

Permanent sync run 188 subsequently checked out exactly `74d8e44f251977585ceccbd87ef1393384d7ae6a`, observed and re-observed every existing GitHub surface, retained the expected `wiki_bootstrap_required`, `discussions_bootstrap_required`, and `projects_auth_required` plan, performed zero Projects mutations, validated the repository, and ended with `No durable GitHub state change.` This proves the richer identity substrate composes cleanly with the no-Projects-credential degraded mode.

## Projects read-only observation progress

Read-only ProjectV2 observation landed as `2af04abb6ec38e5ee78ac2067fb479afa211ed1c`. With separate authorization it can observe the explicitly bound Project or stable mapped node ID and persist normalized Project metadata, fields, single-select options, iterations, views, items, supported field values, and Issue/PullRequest/DraftIssue content. Provider-created DraftIssue content remains GitHub-scoped evidence and is never promoted into canonical issue state. Changed provider snapshots archive both sides before current observation advances; semantically identical re-observation is idempotent, and truncated paginated state is rejected rather than silently persisted.

The observer also harvests ordinary GitHub Issue/PullRequest GraphQL content node IDs only for canonical objects that belong to enabled boards. Permanent sync run 193 checked out the exact observer product commit, observed zero ProjectV2 projects because no separate Projects credential is available, persisted the content-node identities for `issue-0010` and `issue-0011`, re-observed the same two identities idempotently, retained only the expected Wiki/Discussions/Projects capability boundaries, and validated successfully. That durable provider identity state landed in the subsequent sync commit.

## Projects non-destructive mutation runtime progress

The first ProjectV2 mutation runtime landed as `769f2227f0c802b7328ca2038a741cc590a10a25`. Planning now emits at most one mutable ProjectV2 operation per canonical board in a plan, forcing a fresh observation boundary between dependent mutations. Managed targets can create a ProjectV2, create namespaced provider fields, add already-mapped canonical Issues/PullRequests as Project items, reconcile canonical board metadata, and write explicit text, number, date, and single-select field values. Existing targets bind only through their explicitly configured Project number and stable provider identity; missing field identity on an existing Project remains a review/configuration boundary rather than falling back to matching names.

Stable identity is required at every layer. Managed provider fields are created under an `Allodium: <canonical field name>` namespace and their returned node IDs are persisted immediately. Single-select options carry Allodium-owned markers so returned provider option IDs can be mapped without treating option names as identity. Project item membership is created from the already observed Issue/PullRequest GraphQL content node ID, its returned Project item node ID is persisted, and field-value writes use the stable Project/item/field/option identities. Canonical absence still does not clear provider values, and Project/item deletion remains unsupported.

Before every mutable write to an already mapped Project, the runtime rechecks the separately configured Projects credential against the configured owner and then re-fetches the full normalized ProjectV2 state by stable node ID. That live state must exactly match the persisted provider-state observation or the write is refused as stale before a mutation is sent. The runtime gate contains explicit proof that a provider-side change after planning blocks the update, and a separate test proves managed Project creation uses the Projects credential rather than the repository token.

The first mutation gate failed safely before publication because two older planner fixtures modeled only the lightweight Project identity observation and therefore correctly hit the new full provider-state observation fence. The fix did not weaken that fence: the fixtures were upgraded to include normalized provider snapshots, the fully matching fixture now proves an empty idempotent plan, and content identity preflight was made independently diagnosable before provider schema work. The successful self-cleaning gate then passed 81 core tests, 8 checked-in fixture tests, the ingress idempotence test, 45 GitHub adapter tests, formatting, repository validation, and the real no-token plan smoke; the same full suite passed again after rebase before publication.

Permanent sync run 198 checked out exactly `769f2227f0c802b7328ca2038a741cc590a10a25`. It observed all existing GitHub surfaces, zero ProjectV2 projects, and the same two board-member content node identities. Its plan contained only `wiki_bootstrap_required`, `discussions_bootstrap_required`, and `projects_auth_required`. Apply reported zero ProjectV2 creates, binds, observations, updates, field creations, item additions, and field-value writes. Re-observation was clean, validation passed, and the run ended with `No durable GitHub state change.` The non-destructive mutation runtime therefore composes with the repository's real no-Projects-credential degraded mode without widening authority.

The remaining end-to-end Projects acceptance criterion is genuinely credential-dependent: live creation/binding, item projection, field/value reconciliation, deliberate provider drift, archival/repair, and final idempotent re-observation cannot be exercised until a separately authorized `ALLODIUM_GITHUB_PROJECTS_TOKEN` is available. View creation/binding and provider option-schema evolution also remain intentionally outside this first mutation slice and should be specified before they gain mutation authority.

## Projects schema-preservation progress

Provider schema preservation landed as `6d1fee55aa58988165db806cab3c02632c104fe3`. Read-only ProjectV2 observation now retains single-select option name, description, color, and stable option node ID; custom-field provider database IDs as non-authoritative bridge evidence; and richer view state including full database ID, update revision, visible fields, horizontal grouping, vertical grouping, and ordered sort configuration. GraphQL node IDs remain the stable mapping authority throughout.

This boundary exists specifically to make later schema mutation loss-aware. GitHub's Project view APIs split identity and configuration across surfaces: GraphQL carries stable node identity, while the current REST view API can express table/board/roadmap layout, visible fields, sorting, horizontal grouping, and board-column `vertical_group_by` using provider database field IDs. Canonical Allodium views therefore cannot be declared projected from a name match or a partially configured same-name provider view. In particular, canonical board `group_by` must map to provider vertical grouping semantics, while roadmap start/end-field semantics remain deferred until the provider representation can be audited without loss.

Single-select evolution is similarly replacement-shaped. Managed field creation already creates all canonical options in one operation, tags them with Allodium markers, and persists the returned option node IDs. Later option-schema updates must preserve existing provider option identities and foreign options unless an explicit deletion policy says otherwise; canonical absence has no provider-deletion meaning in v0.

The schema-preservation gate failed safely twice before publication. The first failure exposed older planner fixtures that directly constructed provider snapshots without the newly preserved schema fields. Those fixtures were made explicit rather than weakening the provider structs with hidden defaults. The second failure isolated one missing import for the new ordered view-sort evidence type. After both staging defects were corrected, the unchanged self-cleaning gate passed the full locked workspace suite, repository validation, the real no-token plan smoke, and the entire suite again after rebase. The temporary workflow and helper files were removed by the published product commit.

The next safe mutation boundary is view creation/binding for managed Projects, using stable Project/field mappings plus the newly preserved provider bridge identifiers. Existing-target view binding must remain explicit and must not fall back to matching view names. Provider option-schema evolution should remain a separate later slice because its whole-schema replacement semantics require a distinct preservation policy.

## Projects managed view creation progress

Managed lossless ProjectV2 view creation landed as `a175c3442f9116b5fc621a1398f6e731a355d4eb`. The planner now treats a missing canonical view mapping as an actual provider mutation only for explicitly `managed` Projects and only when the canonical view can be represented without semantic loss. Table views are create-once. Board views project canonical `group_by` through the stable provider field node mapping to the corresponding observed provider database field ID and send that bridge identity as GitHub's `vertical_group_by`, preserving Allodium's rule that kanban columns are a presentation over field values rather than canonical column objects.

View identity never comes from matching names. A same-named foreign provider view remains foreign and an unmapped managed canonical view still plans creation. Existing-target views remain a review/configuration boundary until an explicit stable binding design exists. Mapped view name/layout or board-grouping drift is likewise review-only in v0: provider observation archives the change, but Allodium does not silently rewrite provider view configuration or infer deletion from canonical absence.

The user-owned REST view endpoint requires a numeric user identifier even though Allodium's authority identity is the stable GraphQL node ID. Immediately before creating a view, the runtime therefore resolves the configured GitHub user with the separate Projects credential and refuses the write unless the REST `node_id` matches the freshly authorized GraphQL owner node. The numeric ID is used only to address GitHub's REST route and is never promoted into canonical or stable mapping authority. The creation request itself uses the same separate Projects credential, never repository `GITHUB_TOKEN` authority.

Canonical roadmap views remain deliberately deferred. The checked-in `roadmap` view owns `end_field = "target-date"`, but GitHub's currently audited view-creation input does not expose a lossless start/end-field mapping. Allodium therefore refuses to create a superficially similar roadmap and call it projected. Provider view deletion is also unsupported.

The view-runtime gate stopped publication twice while the new fake-provider fixture was malformed. The first failure reached 82 passing core tests and isolated the new adapter test; the second confirmed the attempted fixture correction had not changed the malformed full-Project response. The root cause was one excess closing brace in that fake JSON. The staging repair was changed to parse every fake JSON response before Cargo runs and only repair excess trailing braces when doing so yields valid JSON. With that guard in place, the unchanged self-cleaning gate passed 82 core tests, 8 checked-in fixture tests, the ingress idempotence test, and 47 GitHub adapter tests, then repeated the entire suite after rebase before publishing the clean product commit. The real repository no-token plan still contained only `wiki_bootstrap_required`, `discussions_bootstrap_required`, and `projects_auth_required`, with no ProjectV2 mutation.

The remaining end-to-end Projects acceptance criterion is still credential-dependent and remains open. The next independent schema-mutation question is preservation-aware evolution of managed single-select options; current field creation is complete, but later whole-schema option replacement must not erase foreign provider options or silently assign deletion meaning to canonical absence.

## Design constraints

This slice should resist two opposite failures. It must not reduce Allodium to a lowest-common-denominator fake forge API, but it also must not make GitHub's flexible ProjectV2 schema the ontology of the project itself. Provider richness can live in the GitHub adapter and remote namespace while the canonical board remains directly understandable and editable with ordinary filesystem tools.

The implementation sequence is canonical board core, then provider capability/configuration, then provider identity/mapping and observation, and only then ProjectV2 mutation. Each layer must remain useful and valid when the next provider capability is unavailable.

This issue is canonical Allodium state. Any GitHub Issue projection of this work item is downstream.

---

**Allodium canonical ID:** `issue-0011`

This GitHub issue is a projection of `.project/issues/issue-0011/`, not the canonical record.