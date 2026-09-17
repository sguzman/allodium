# Roadmap

Allodium is intentionally starting GitHub-first while keeping the filesystem model sovereign.

## M0 — Constitutional substrate

Goal: freeze enough of the philosophy and filesystem contract that implementation cannot accidentally drift into forge-owned or Git-owned state.

- [x] Define sovereignty and filesystem-native invariants.
- [x] Establish `.project/` layout.
- [x] Define canonical / configuration / incoming / reconstructible state classes.
- [x] Define first project, issue, and remote records.
- [x] Bootstrap validator CLI.
- [x] Dogfood Allodium's own repository state.
- [x] Add validator and reconciliation unit tests.
- [x] Add file-backed valid and invalid fixtures.
- [x] Specify review, milestone, label, wiki, and release records.
- [x] Define versioning/migration policy before declaring v1 formats.

M0 is complete. The filesystem contract has executable checked-in fixtures, explicit schema-evolution rules, and forge-neutral v0 records for the next canonical object families.

## M1 — GitHub issues vertical slice

Goal: one canonical issue can be projected to GitHub and externally originated issue activity can return to the repository with provenance.

- [x] GitHub authentication boundary with non-persistent environment credentials.
- [x] Canonical issue create/update/close API implementation.
- [x] Stable mapping files.
- [x] Define exact observed issue snapshot format, including body.
- [x] Fetch/refresh remote issue observations through a dedicated GitHub adapter.
- [x] Define and implement idempotent external-comment provenance archival.
- [x] Dogfood one real GitHub comment into the incoming filesystem archive.
- [x] Capture externally changed managed fields without silently accepting them as canonical.
- [x] Dry-run plan output before mutations.
- [x] Idempotent reconciliation tests.
- [x] Explicit plan-file apply with optimistic GitHub revision guard.
- [x] Dogfood live adapter observation and plan application end-to-end.
- [x] Prove filesystem-only canonical issue creation, update, and close through the automatic GitHub projection workflow.
- [x] Archive GitHub-created unmapped issues as provider-scoped evidence without silently creating canonical issues or mappings.
- [x] Archive previously observed comments that disappear from the REST listing as `no_longer_observed` evidence without asserting a deletion actor or deletion timestamp.
- [x] Add HTTP-level stale-write coverage with a local fake server and prove no PATCH is sent after a revision mismatch.

M1 is complete. Remaining GitHub issue work belongs either to hardening discovered by later dogfood or to broader object types in subsequent milestones.

## M2 — Reviews / pull requests

- [x] Canonical review format.
- [x] Project review to GitHub PR.
- [x] PR conversation ingestion.
- [x] Review submission ingestion.
- [x] Inline review thread ingestion.
- [x] Merge/close state reconciliation.
- [x] Preserve GitHub-local social state without cross-forge impersonation.

M2 is complete. A real canonical review was projected to GitHub PR #7, social/review ingress was archived as provider-scoped evidence, canonical close was reconciled outward, external reopen was archived then overridden by canonical state, and merged PR state is preserved as terminal remote evidence rather than treated as canonical merge identity.

## M3 — Wiki and releases

### Wiki

- [x] Make the canonical `.project/wiki/` metadata/home contract executable.
- [x] Derive a deterministic, network-free GitHub Wiki projection plan.
- [x] Persist observed wiki branch, HEAD revision, and exact file tree.
- [x] Guard outbound wiki writes against stale provider revisions; never force-push.
- [x] Preserve provider-only wiki tree changes as GitHub-scoped evidence.
- [x] Define an explicit v0 asset/path policy: top-level Markdown only, unsupported assets/nesting fail loudly.
- [ ] Project the real Allodium canonical wiki to GitHub Wiki and re-observe idempotently.

The final live wiki item is currently blocked by a GitHub bootstrap prerequisite rather than by Allodium reconciliation. GitHub does not expose the separate `allodium.wiki` Git repository until an initial page has been created on GitHub; before that initialization the repository resource is absent. Allodium deliberately does not work around this with undocumented provider endpoints. The permanent reconciliation plan surfaces this as the non-mutating `wiki_bootstrap_required` operation so the blocked Wiki surface cannot prevent issues, reviews, or releases from reconciling.

### Releases

- [x] Make canonical release records and notes executable in validation.
- [x] Define Git tag/revision relationship rules for the GitHub v0 adapter.
- [x] Add network-free GitHub Release planning plus provider-scoped mappings/observations.
- [x] Add GitHub Release create/update runtime with optimistic stale-state refusal.
- [x] Preserve provider-side managed release changes as incoming evidence.
- [x] Dogfood one canonical Allodium release into a real GitHub draft release and re-observe idempotently.

The Release vertical slice is complete. Canonical `release-0001` projected to a real GitHub draft release and exact Git tag/revision, survived a deliberate provider-only title edit, archived that drift before repair, and converged to a no-op Release plan on a clean helper-free reconciliation run. Live dogfood exposed a GitHub draft-release quirk: PATCHing mutable fields can replace the Release object's `tag_name` with an internal `untagged-*` attachment slug even while the real Git ref remains intact. Allodium therefore treats the mapped Git tag ref as the immutable provider identity anchor and the Release object's `tag_name` as provider attachment state that is observed, archived when it drifts, and repaired by reasserting canonical `tag_name` plus `target_commitish` on mutable updates.

## M4 — Broader GitHub surface

- [x] Milestones as first-class canonical objects with live GitHub create/update/close projection, stale-write protection, provider-drift archival/repair, and idempotent dogfood.
- [x] Labels as first-class canonical objects.
- [ ] Discussions where API semantics are sufficient.
- [ ] Project/board model after careful ontology work.
- [ ] Better webhook/event-driven ingress where useful.

The Milestone vertical slice is complete. Canonical `milestone-0001` projected to GitHub Milestone #1, survived deliberate provider-only title drift with preserved before/after evidence and canonical repair, and closed downstream only after canonical state changed to `closed`. Live dogfood also established the GitHub `due_on` boundary: absent canonical due dates omit the provider field, present dates use a deterministic end-of-day UTC representation, and v0 refuses to guess an undocumented null-based due-date clear.

The Label vertical slice is complete. Canonical `label-allodium` projected to GitHub label ID `12237280577`, uses that stable provider ID rather than mutable label name as its mapping anchor, survived a deliberate provider-only rename with preserved before/after evidence, repaired the canonical name through the normal plan/apply path, and converged to a no-op label plan. Because GitHub labels expose no `updated_at` revision, v0 update safety re-lists by stable provider ID and requires exact equality with the last observed managed snapshot before PATCH. Issue↔label membership and destructive label deletion remain intentionally deferred until Allodium canonically owns the affected association semantics.

## M5 — Second forge

Only after GitHub proves the canonical boundary.

- Implement a second adapter, likely Forgejo or GitLab.
- Identify accidental GitHub assumptions in canonical schemas.
- Correct those assumptions without reducing GitHub support to a lowest common denominator.
- Demonstrate one canonical project projecting to two isolated remote authorities.

## Non-goals for now

- replacing Git itself;
- mirroring forge social metrics unrelated to project operational state;
- hiding all provider-specific differences behind one fake universal API;
- automatic cross-forge reposting of user-generated comments;
- a GUI before the filesystem and reconciliation contracts are trustworthy.
