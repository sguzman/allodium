# GitHub Projects provider identity v0

Allodium boards remain canonical under `.project/boards/`. GitHub ProjectV2 state is a provider projection and all ProjectV2 identity belongs under `.project/remotes/github/`.

## Projection target configuration

`.project/remotes/github/projects.toml` declares an explicit owner authority and an explicit target policy for each canonical board. `target = "managed"` means Allodium may eventually create and manage a provider ProjectV2 after explicit planning. `target = "existing"` requires `project_number` and means the user selected that provider object intentionally. Allodium never binds a Project by matching a title.

The file stores only the environment-variable name used for Projects credentials. Credential values are never serialized.

## Stable provider mapping

`mappings/projects.toml` is reserved for stable ProjectV2 identity. A board mapping may contain the provider Project number/node ID/URL/owner node ID and canonical-to-provider mappings for Project items, fields, single-select options, and views. These identities never appear in canonical board files.

## Observation

`observed/projects/boards/<board-id>.toml` records the last observed mutable ProjectV2 surface used for optimistic planning. `observed/projects/content/<canonical-id>.toml` records the GraphQL node identity of an already-mapped GitHub issue or pull request because `addProjectV2ItemById` requires the content node ID rather than the repository issue number.

Provider-created DraftIssue items are not canonicalized by this mapping layer. Foreign Project activity belongs in provider evidence/observation state until a later promotion policy explicitly says otherwise.

## Non-destructive mutation boundary

The first ProjectV2 mutation runtime is deliberately staged. Planning emits at most one mutable ProjectV2 operation per canonical board, so every project creation, provider-field creation, item-membership addition, metadata update, or field-value update is separated by a fresh observation boundary. This matches GitHub's own API contract that adding an item and updating its field values are separate mutations.

Managed targets may create ProjectV2 projects and provider fields. Allodium-created provider fields are namespaced as `Allodium: <canonical name>` and immediately receive stable field/option mappings; this avoids guessing identity from GitHub field names or accidentally adopting provider defaults. Existing targets bind only by the explicitly configured Project number plus stable node identity. Missing field identity on an existing target remains a review/configuration boundary rather than a name-match heuristic.

Before every mutable write to an already-mapped Project, the runtime re-fetches the full normalized ProjectV2 state by stable node ID and compares it to the persisted provider-state observation. Any intervening provider change causes a stale-state refusal before mutation. Explicit canonical item values are managed only after both Project item and field identities exist; canonical absence does not clear provider values in v0. Project/item deletion remains unsupported, and view creation/binding remains a later non-destructive slice.


## Read-only provider observation

When the separately configured Projects credential authenticates the explicit owner, Allodium may observe an explicitly numbered existing Project or a Project already bound by stable node ID. The v0 observer persists normalized provider snapshots for project metadata, fields, single-select options, iterations, views, items, supported field values, and Issue/PullRequest/DraftIssue content. Provider DraftIssue content remains provider evidence and is never promoted into `.project/issues/`.

A changed provider snapshot archives both the previous and current observations under the GitHub remote `incoming/` namespace before the current observation is replaced. Re-observing semantically identical state is idempotent. Project connections are capped at 100 objects in v0; if GitHub reports another page, observation fails rather than silently persisting a truncated snapshot.

Ordinary mapped issue and pull-request observations also persist their GitHub GraphQL node IDs when those canonical objects belong to an enabled Allodium board. This supplies the stable content identity required for later ProjectV2 item membership without requiring Project authorization merely to learn repository-object identity.

## Provider schema-preservation boundary

ProjectV2 schema mutation is intentionally narrower than provider observation. Allodium now preserves enough provider-only schema detail to make a later mutation decision without reconstructing identity from display names or discarding foreign state:

- single-select option ID, name, description, and color;
- custom-field GraphQL node ID plus the provider database ID as non-authoritative bridge evidence;
- view node ID, number, full database ID, update revision, visible field IDs, horizontal group-by field IDs, vertical group-by field IDs, and ordered sort field/direction pairs.

GraphQL node IDs remain the stable provider identities. Numeric/full database IDs are evidence for provider API bridges only and must never replace node IDs as mapping authority.

The distinction matters because GitHub's GraphQL view mutation input can configure visible field IDs but does not expose the richer grouping/sort shape that canonical Allodium views may require. GitHub's REST Project-view surface can express group-by, vertical-group-by, visible fields, and sorting, but it uses database identifiers and has owner/credential-specific support. A canonical board view therefore must not be declared projected merely because a same-named GitHub view exists or because a partial GraphQL view was created. In particular, Allodium's canonical board `group_by` describes kanban columns and must round-trip to the provider's vertical grouping semantics; the canonical roadmap start/end-field semantics must remain deferred until an audited provider mutation can represent them without loss.

Single-select option evolution has a similar whole-schema hazard. GitHub's field-update mutation treats the submitted option list as replacement configuration, and existing provider option IDs must be supplied to preserve option identity and item values. Allodium therefore observes the complete option identity/details before any option-schema update is allowed. Foreign provider options are evidence, not canonical options, and canonical option absence has no deletion meaning in v0.
