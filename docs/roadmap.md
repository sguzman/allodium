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
- [ ] Add file-backed invalid fixtures.
- [ ] Specify review, milestone, label, wiki, and release records.
- [ ] Define versioning/migration policy before declaring v1 formats.

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

- Canonical review format.
- Project review to GitHub PR.
- PR conversation ingestion.
- Review submission ingestion.
- Inline review thread ingestion.
- Merge/close state reconciliation.
- Preserve GitHub-local social state without cross-forge impersonation.

## M3 — Wiki and releases

- Canonical `.project/wiki/` projection to GitHub Wiki.
- Wiki asset strategy.
- Canonical release records and notes.
- Git tag/revision relationships.
- GitHub Release projection.

## M4 — Broader GitHub surface

- Milestones and labels as first-class canonical objects.
- Discussions where API semantics are sufficient.
- Project/board model after careful ontology work.
- Better webhook/event-driven ingress where useful.

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
