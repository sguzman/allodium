# Canonical object records v0

This document specifies the first filesystem records for canonical project objects beyond issues. These are storage contracts, not promises that every adapter already projects them.

All canonical IDs are project-owned. Provider numbers, node IDs, URLs, and other forge identifiers belong under remote mappings or observations, never in canonical identity fields.

## Reviews

Canonical concept: a proposed/reviewed change, not a GitHub Pull Request or GitLab Merge Request.

```text
.project/reviews/review-0001/
  review.toml
  body.md
```

`review.toml`:

```toml
schema = "allodium.review/v0"
id = "review-0001"
title = "Rewrite renderer"
state = "open"
base = "main"
head = "renderer-rewrite"
```

Required fields:

- `id`: stable canonical review ID; must match the directory name.
- `title`: human-facing title.
- `state`: `open`, `merged`, or `closed`.
- `base`: project ref/revision expression the change targets.
- `head`: project ref/revision expression containing the proposed change.

`body.md` contains canonical review description prose.

A future revision may add immutable base/head revision snapshots, canonical review participants, and merge-result metadata. Provider review conversations remain provider-scoped ingress unless explicitly promoted.

## Milestones

```text
.project/milestones/milestone-0001/
  milestone.toml
  description.md
```

`milestone.toml`:

```toml
schema = "allodium.milestone/v0"
id = "milestone-0001"
title = "M2 reviews"
state = "open"
due = "2026-12-01"
```

Required fields:

- `id`: stable canonical milestone ID; must match the directory name.
- `title`: human-facing milestone title.
- `state`: `open` or `closed`.

Optional fields:

- `due`: ISO 8601 calendar date. Absence means no canonical due date.

`description.md` contains milestone prose. Provider milestone numbers are mappings, not canonical IDs.

## Labels

```text
.project/labels/label-bug.toml
```

```toml
schema = "allodium.label/v0"
id = "label-bug"
name = "bug"
description = "Something is not behaving as intended."
color = "#d73a4a"
```

Required fields:

- `id`: stable canonical label identity and filename stem.
- `name`: human-facing label text.

Optional fields:

- `description`: prose definition.
- `color`: presentation hint in `#RRGGBB` form.

The label `id` is identity. The display name may be renamed without changing identity. Provider label IDs/names are projections and mappings.

## Wiki

Canonical wiki content is an ordinary directory:

```text
.project/wiki/
  wiki.toml
  index.md
  architecture.md
  assets/
```

`wiki.toml`:

```toml
schema = "allodium.wiki/v0"
title = "Allodium Wiki"
home = "index.md"
```

Required fields:

- `title`: human-facing wiki title.
- `home`: relative path to the canonical home page under `.project/wiki/`.

Markdown pages and assets are canonical files. The metadata file does not enumerate every page; the directory is the page inventory. Provider wiki repositories are projection targets, never the sole copy.

Paths inside the wiki are project-owned addresses. An adapter may need provider-specific escaping or rewriting while projecting them.

## Releases

```text
.project/releases/release-0001/
  release.toml
  notes.md
```

`release.toml`:

```toml
schema = "allodium.release/v0"
id = "release-0001"
title = "0.1.0"
version = "0.1.0"
state = "published"
revision = "0123456789abcdef"
tag = "v0.1.0"
```

Required fields:

- `id`: stable canonical release ID; must match the directory name.
- `title`: human-facing release title.
- `version`: project-defined version string.
- `state`: `draft` or `published`.
- `revision`: Git revision the release describes.

Optional fields:

- `tag`: desired or associated Git tag name.

`notes.md` contains canonical release notes.

Release binaries are not required to live in `.project/`. Artifact manifests, checksums, and storage policies can be added separately. GitHub Release IDs and asset URLs are remote state.

## Cross-object references

Canonical objects refer to each other by canonical IDs, never by GitHub/GitLab/Forgejo identifiers. A later schema may add fields such as:

```toml
milestone = "milestone-0002"
labels = ["label-bug", "label-renderer"]
```

Validators should reject references to missing canonical IDs once those relationships become part of an implemented schema.

## Authority classes

Everything specified here is canonical project state. Corresponding provider mappings and observations remain under:

```text
.project/remotes/<remote>/mappings/
.project/remotes/<remote>/observed/
.project/remotes/<remote>/incoming/
```

An adapter may project only a subset of canonical fields if a provider lacks equivalent semantics. That limitation belongs to the adapter; it does not redefine the canonical record.

## Manual editing remains valid

These records are deliberately small TOML files paired with Markdown where prose is needed. A user may create, rename, or edit them with ordinary filesystem tools. Allodium's job is to validate and project that state, not to hide ownership behind a database or provider API.
