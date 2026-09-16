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

---

**Allodium canonical ID:** `issue-0008`

This GitHub issue is a projection of `.project/issues/issue-0008/`, not the canonical record.