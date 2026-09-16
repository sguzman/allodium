# Validator fixtures

These directories are intentionally checked-in filesystem projects used as black-box validator inputs.

They are not generated during the test. That is deliberate: Allodium's storage contract is ordinary files and directories, so tests should exercise durable example trees that a person can inspect and edit with normal filesystem tools.

- `valid-minimal/` is the smallest accepted project with one issue and one remote.
- `invalid-issue-id/` has an issue ID that disagrees with its directory name.
- `invalid-issue-state/` uses an unsupported issue state.
- `invalid-project-schema/` uses an unsupported project schema version.
- `valid-review/` adds one canonical review with distinct base and head refs.
- `invalid-review-same-ref/` uses the same ref for both sides of a review.

When a validation rule changes, update or add a fixture that makes the filesystem-level contract explicit.
