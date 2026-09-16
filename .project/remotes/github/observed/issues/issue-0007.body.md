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

## Completion evidence

- Canonical `release-0001` projected to GitHub Release ID `390189282` as a draft with tag `v0.0.0-m3-dogfood` and immutable revision `33d9e4b4d0199bfa29deb7bafa34fd82a486ca36`.
- The Git tag `refs/tags/v0.0.0-m3-dogfood` resolves to that exact canonical revision and is treated as the immutable provider identity anchor.
- A deliberate provider-only title edit exposed GitHub draft behavior that rewrote the Release object's attachment `tag_name` to an internal `untagged-*` slug while leaving the real Git tag intact.
- Allodium now distinguishes those surfaces: the mapped Git tag ref anchors immutable identity, while Release `tag_name` is observed provider attachment state that may drift and be repaired.
- The provider drift was archived under `.project/remotes/github/incoming/` with before/after snapshots and `fields = ["title", "tag"]`, then canonical title/tag attachment were restored automatically.
- Mutable Release PATCH operations reassert canonical `tag_name` and `target_commitish`, preventing GitHub's draft machinery from silently detaching future updates from canonical release identity.
- The final helper-free reconciliation observed one mapped release, archived zero new release changes, planned no Release operation, applied `created 0 / updated 0 / observed 0` Releases, and committed no durable GitHub state change. The only remaining M3 plan operation was the unrelated non-mutating `wiki_bootstrap_required` capability boundary.

This issue is canonical Allodium state. GitHub Releases, GitHub release IDs, generated archive URLs, and provider-side release metadata are projections around `.project/releases/`, not the owner of release intent.

---

**Allodium canonical ID:** `issue-0007`

This GitHub issue is a projection of `.project/issues/issue-0007/`, not the canonical record.