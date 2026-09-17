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
