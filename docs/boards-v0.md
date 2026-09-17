# Canonical boards v0

Allodium boards are project-owned filesystem state. GitHub Projects, GitLab boards, Forgejo boards, and future local UIs are projections of that state rather than its authority.

The v0 model deliberately separates **data** from **presentation**:

```text
.project/boards/board-0001/
├── board.toml
├── fields/
│   ├── status.toml
│   └── target-date.toml
├── items/
│   ├── issue-0010.toml
│   └── issue-0011.toml
└── views/
    ├── development.toml
    └── roadmap.toml
```

Every authoritative artifact is an ordinary TOML file. No database, Git ref, provider object, or Allodium executable is required to inspect or edit the board.

## Board identity

`board.toml` owns the board's project-level identity:

```toml
schema = "allodium.board/v0"
id = "board-0001"
title = "Allodium development"
description = "Dogfood board for active Allodium work."
```

The `id` must match the containing directory.

## Fields

Fields are typed project-owned metadata definitions. v0 supports:

- `text`
- `number`
- `date`
- `single_select`

A single-select field owns stable option IDs:

```toml
schema = "allodium.board-field/v0"
id = "status"
name = "Status"
kind = "single_select"

[[options]]
id = "active"
name = "Active"

[[options]]
id = "blocked"
name = "Blocked"
```

Items store `active` or `blocked`, not the display label. Renaming `Blocked` therefore does not silently rewrite item identity.

Only single-select fields may declare options. Dates are strict ISO 8601 calendar dates (`YYYY-MM-DD`).

## Items

An item is membership of an existing canonical Allodium object in a board:

```toml
schema = "allodium.board-item/v0"
object = "issue-0011"

[values]
status = "active"
target-date = "2026-10-01"
```

The filename stem must equal the canonical object ID. v0 accepts objects from canonical families Allodium already owns, initially issues and reviews. A provider DraftIssue is not a canonical Allodium issue merely because GitHub Projects can contain it.

Values must reference declared fields and conform to their types.

## Views

Views are presentation metadata over board data:

```toml
schema = "allodium.board-view/v0"
id = "development"
name = "Development"
layout = "board"
group_by = "status"
```

v0 layouts are:

- `table`
- `board`
- `roadmap`

A board view requires `group_by`. Its visible columns are the grouped field values. **Columns are not canonical objects.**

A roadmap view requires at least one of `start_field` or `end_field`, and those references must point at canonical date fields.

This distinction is intentional. The board's items and values remain the same when a user switches among table, kanban, and roadmap presentations.

## Validation invariants

Allodium rejects:

- board IDs that do not match their directory;
- field or view IDs that do not match filename stems;
- unsupported field or view kinds;
- duplicate field, view, or single-select option IDs;
- single-select fields without options;
- options on non-single-select fields;
- board items whose filename and canonical object differ;
- board items referencing missing canonical issues/reviews;
- duplicate membership of the same canonical object in one board;
- values referencing unknown fields;
- values incompatible with their field type;
- single-select values that are not declared option IDs;
- view field references that do not exist;
- board views without a grouping field;
- roadmap date references that are not date fields.

## GitHub Projects boundary

GitHub's current Projects model is a flexible collection of items, fields, and views scoped to a user or organization. A GitHub board view is itself a presentation over Project fields; its columns do not need to become Allodium's ontology.

The GitHub adapter will therefore keep provider-specific identity under `.project/remotes/github/`, including:

- Project node ID/number and URL;
- owner authority and whether it is a user or organization;
- Project item node IDs;
- field and option/iteration node IDs;
- view node IDs.

GitHub Project DraftIssues remain provider-scoped unless explicitly promoted into a canonical Allodium object.

GitHub also documents a distinct authentication boundary: repository Actions `GITHUB_TOKEN` cannot access Projects. A later adapter increment must use a separately authorized GitHub App or personal access token, keep that credential outside `.project/`, and surface missing authority as a non-mutating capability requirement instead of blocking unrelated repository reconciliation.

## v0 scope

This first increment is intentionally canonical-only. It establishes executable filesystem semantics before provider mapping, observation, planning, or mutation exists. That ordering is part of the sovereignty model: GitHub ProjectV2 should adapt to Allodium's project state, not define it.
