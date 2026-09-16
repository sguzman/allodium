Finish the remaining M0 substrate work so future adapters cannot outrun the filesystem contract.

## Acceptance criteria

- Add checked-in valid and invalid `.project/` fixture trees.
- Exercise those fixture directories through the real validator.
- Define schema versioning and migration rules, including fail-closed unknown semantics and explicit filesystem migrations.
- Specify v0 canonical records for reviews, milestones, labels, wiki metadata, and releases.
- Keep every authoritative record manually inspectable/editable as ordinary files.
- Mark M0 complete only after CI validates the new fixtures and documentation lands.

This issue is canonical Allodium state. Its GitHub projection may have a different numeric issue ID; the canonical identity is `issue-0004`.
