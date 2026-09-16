# Filesystem format v0

This document defines the first experimental Allodium layout. `v0` remains explicitly versioned even while it is experimental. Schema evolution and migration rules are normative in `format-versioning.md`; the v0 records for reviews, milestones, labels, wiki metadata, and releases are normative in `canonical-object-records-v0.md`.

## Project manifest

Path: `.project/manifest.toml`

```toml
schema = "allodium.project/v0"
id = "example-project"
name = "Example Project"
description = "Optional prose."
```

`id` is the stable project identity inside Allodium. Renaming a repository or moving between forges must not require changing it.

## Issues

Each issue occupies one directory:

```text
.project/issues/issue-0001/
  issue.toml
  body.md
```

`issue.toml`:

```toml
schema = "allodium.issue/v0"
id = "issue-0001"
title = "Renderer hangs after resize"
state = "open"
labels = ["bug", "renderer"]
```

The directory name and `id` must match. `body.md` contains only the issue body; the title remains a separate canonical field and should not be repeated as a synthetic heading. Keeping prose separate makes manual editing pleasant and avoids frontmatter tooling.

Future issue subobjects may include canonical notes, attachments, relationships, and explicit provenance links.

## Reviews / proposed changes

The canonical concept is a `review`, not a GitHub Pull Request or GitLab Merge Request.

The v0 review record is specified in `canonical-object-records-v0.md` and uses ordinary files:

```text
.project/reviews/review-0001/
  review.toml
  body.md
```

A review identifies project-owned base/head refs or revisions plus canonical review state. GitHub may project it as a pull request while another forge may project the same canonical object differently.

## Milestones, labels, wiki, and releases

The canonical v0 layouts and fields for milestones, labels, wiki metadata/content, and releases are specified in `canonical-object-records-v0.md`. Provider-assigned numbers, IDs, URLs, and remote asset metadata never become canonical identity merely because an adapter projects these objects.

## Wiki

Canonical wiki content lives under `.project/wiki/` as ordinary Markdown and assets. The GitHub adapter may project that content into the repository's GitHub Wiki Git repository or another supported GitHub-facing representation.

The canonical wiki never lives only in `project.wiki.git`.

## Remote configuration

Path: `.project/remotes/<remote>/remote.toml`

```toml
schema = "allodium.remote/v0"
name = "github"
kind = "github"
repository = "owner/repository"
outbound = "reconcile"
inbound = "archive"
```

`outbound = "reconcile"` means canonical state is the desired state for provider-owned fields that Allodium manages.

`inbound = "archive"` means externally originated activity is captured locally but does not automatically mutate canonical state.

## Remote directory classes

```text
.project/remotes/github/
  remote.toml
  mappings/   # reconstructible identity relationships
  observed/   # reconstructible remote snapshot
  incoming/   # provenance-preserving remote-originated history
```

### GitHub issue mappings

```text
.project/remotes/github/mappings/issues.toml
```

```toml
schema = "allodium.github.issue-mappings/v0"

[issues.issue-0001]
number = 37
url = "https://github.com/owner/repository/issues/37"
```

The mapping is not identity. `issue-0001` remains the project-owned identity; `37` is the identifier assigned by one projection authority.

### GitHub observed issues

The current remote observation is stored as ordinary files:

```text
.project/remotes/github/observed/issues/
  issue-0001.toml
  issue-0001.body.md
  issue-0001.revision.toml
```

The metadata file stores observed remote fields, the Markdown file stores the exact observed GitHub body, and the revision file stores the provider revision used for optimistic stale-write protection. Prose remains separate so a person can inspect and diff it naturally.

Observed state is reconstructible. Deleting it must never destroy canonical project information.

### Reconciliation plans

A GitHub dry-run plan is serializable TOML using `allodium.github.plan/v0`. It contains explicit operations such as `create_issue` and `update_issue`, their canonical IDs, relevant GitHub numbers, managed fields, and human-readable reasons.

Plans are derived artifacts. They make intended remote mutations inspectable; they are not another source of truth.

### Incoming event envelope

Normalized incoming evidence is stored as ordinary files:

```text
incoming/2026/09/<event-id>/
  event.toml
  body.md       # when the event has prose
  raw.json      # optional provider payload
```

Example `event.toml`:

```toml
schema = "allodium.incoming-event/v0"
id = "github-issue-comment-98214321-created"
remote = "github"
kind = "issue.comment.created"
observed_at = "2026-09-16T12:00:00Z"

[target]
canonical_id = "issue-0001"
remote_type = "issue"
remote_id = "37"

[actor]
remote_id = "284928"
login = "some-user"

[source]
remote_object_type = "issue_comment"
remote_object_id = "98214321"
created_at = "2026-09-16T11:59:55Z"
updated_at = "2026-09-16T11:59:55Z"
url = "https://github.com/owner/repository/issues/37#issuecomment-98214321"
```

Provider identity remains provider-scoped. Matching usernames on two forges are not assumed to denote the same person. Evidence semantics, including unmapped issues and comments no longer observed, are specified in `ingress-and-provenance.md`.

## Canonical versus reconstructible

A file being checked into the repository does not automatically make it canonical. Allodium documents the semantic class of each subtree:

- canonical: project-owned meaning;
- configuration: project-owned instructions for projection;
- incoming: durable foreign-originated provenance;
- reconstructible: mappings/observations/caches that can be rebuilt.

All of them remain ordinary user-editable files. The distinction governs reconciliation semantics, not filesystem permissions.

## Versioning and migrations

Structured records identify their own schema versions inside the ordinary file. Unknown semantic versions fail closed. Reading, validation, observation, planning, and projection do not implicitly migrate canonical state.

A migration is an explicit, reviewable filesystem mutation. See `format-versioning.md` for the complete policy.
