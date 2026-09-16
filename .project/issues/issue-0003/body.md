M1 has a working GitHub issue projection loop. Harden it by converting dogfood findings into explicit behavior and tests rather than relying on the happy path.

## Scope

- Prove canonical issue creation through the automatic GitHub sync workflow.
- Add regression coverage for representation-only body differences.
- Define comment deletion capture semantics.
- Define policy for GitHub-created issues that have no canonical mapping.
- Add stale-write guard coverage.
- Keep remote-derived evidence provenance-preserving and provider-scoped.

This issue is canonical Allodium state. Its GitHub issue is only a projection.
