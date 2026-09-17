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

## Current mutation boundary

This increment is identity/planning only. The planner can distinguish managed creation, binding an explicitly numbered existing Project, missing observation, content identity prerequisites, item/field/option/view mapping gaps, and identity disagreement. All such work still emits the non-mutating `projects_runtime_required` boundary. No ProjectV2 deletion is planned, and canonical absence has no provider deletion meaning yet.


## Read-only provider observation

When the separately configured Projects credential authenticates the explicit owner, Allodium may observe an explicitly numbered existing Project or a Project already bound by stable node ID. The v0 observer persists normalized provider snapshots for project metadata, fields, single-select options, iterations, views, items, supported field values, and Issue/PullRequest/DraftIssue content. Provider DraftIssue content remains provider evidence and is never promoted into `.project/issues/`.

A changed provider snapshot archives both the previous and current observations under the GitHub remote `incoming/` namespace before the current observation is replaced. Re-observing semantically identical state is idempotent. Project connections are capped at 100 objects in v0; if GitHub reports another page, observation fails rather than silently persisting a truncated snapshot.

Ordinary mapped issue and pull-request observations also persist their GitHub GraphQL node IDs when those canonical objects belong to an enabled Allodium board. This supplies the stable content identity required for later ProjectV2 item membership without requiring Project authorization merely to learn repository-object identity.
