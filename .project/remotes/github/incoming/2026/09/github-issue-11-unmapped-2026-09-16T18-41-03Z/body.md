Implement the first canonical release projection vertical slice without making GitHub Releases or Git tags the canonical release record.

## Acceptance criteria

- Canonical `.project/releases/release-*/release.toml` and `notes.md` load and validate without GitHub knowledge.
- Release identity is the canonical Allodium ID; GitHub release IDs and URLs remain remote mappings/observations.
- Canonical `revision` remains project-owned release intent. The GitHub v0 adapter resolves and verifies the release tag against that immutable commit before mutation.
- GitHub projection requires an explicit canonical `tag`; a missing tag is a provider limitation error, not an implicit provider-generated canonical choice.
- Projection planning is inspectable and network-free.
- Mappings and observed GitHub Release state are persisted under `.project/remotes/github/`.
- Creating a GitHub Release never reuses an existing tag that resolves to a different commit.
- Mutable managed fields (title, notes, draft/published state) reconcile outward while tag/revision identity changes fail loudly in v0 rather than silently retargeting history.
- Applying an update refuses to mutate after the live GitHub Release differs from the recorded observation.
- Provider-side managed-field changes are preserved as GitHub-scoped incoming evidence before reconstructible observed state advances.
- At least one canonical Allodium release is projected to a real GitHub draft release and re-observed idempotently.
- Published-state behavior is covered by adapter tests; live publication is not required merely to prove the draft projection slice.

This issue is canonical Allodium state. GitHub Releases, GitHub release IDs, generated archive URLs, and provider-side release metadata are projections around `.project/releases/`, not the owner of release intent.

---

**Allodium canonical ID:** `issue-0007`

This GitHub issue is a projection of `.project/issues/issue-0007/`, not the canonical record.