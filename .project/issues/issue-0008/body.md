Implement the first canonical milestone projection vertical slice without making GitHub Milestones the owner of milestone identity or lifecycle.

## Acceptance criteria

- Canonical `.project/milestones/milestone-*/milestone.toml` and `description.md` load and validate without GitHub knowledge.
- Milestone IDs are project-owned and must match their directory names; GitHub milestone numbers/URLs remain remote mappings and observations.
- Canonical milestone state is `open` or `closed`; optional `due` dates are validated as ISO 8601 calendar dates.
- Planning is deterministic and network-free.
- GitHub mappings and observed milestone snapshots live under `.project/remotes/github/`.
- Create/update/close reconcile outward through the normal inspectable plan/apply path.
- Applying an update refuses to mutate after live GitHub milestone state differs from the recorded observation.
- Provider-side managed milestone changes are archived as GitHub-scoped incoming evidence before reconstructible observation advances.
- A real canonical `milestone-0001` is projected to GitHub and re-observed idempotently.
- Attaching canonical issues to milestones is explicitly out of scope until the issue schema gains a canonical milestone reference; provider attachment must not invent that relationship.

The dogfood object for this slice is `.project/milestones/milestone-0001/`. This issue is canonical Allodium state; any GitHub Issue projection is downstream.

## Live dogfood notes

The first live create attempt reached GitHub through the permanent Allodium sync and exposed a provider contract edge: GitHub rejected `due_on: null` with HTTP 422 (`nil is not a string`). Allodium now omits `due_on` entirely when canonical `due` is absent, while a present canonical date projects deterministically as end-of-day UTC (`YYYY-MM-DDT23:59:59Z`). The documented GitHub milestone REST contract exposes `due_on` as a timestamp string rather than a nullable field, so v0 refuses to clear an existing provider due date by emitting undocumented JSON null; that limitation remains explicit instead of being guessed around.
