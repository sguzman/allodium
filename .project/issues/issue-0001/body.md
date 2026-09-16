# Complete M0 filesystem format and validator

Allodium needs a sufficiently explicit experimental format that the GitHub adapter cannot accidentally become the real data model.

## Acceptance criteria

- Canonical object layouts are documented.
- Format versioning/migration rules are documented.
- Validator has positive and negative fixtures.
- Reviews, milestones, labels, wiki metadata, and releases have v0 records.
- The repository validates its own `.project/` tree in CI.
