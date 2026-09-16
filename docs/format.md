# Filesystem format v0

This document defines the first experimental Allodium layout. `v0` is intentionally allowed to change while M0 is active.

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

The directory name and `id` must match. The prose body is separate so it remains pleasant to edit without frontmatter tooling.

Future issue subobjects may include canonical notes, attachments, relationships, and explicit provenance links.

## Reviews / proposed changes

The canonical concept is a `review`, not a GitHub Pull Request or GitLab Merge Request.

Planned layout:

```text
.project/reviews/review-0001/
  review.toml
  body.md
```

A review will identify a base and head revision/ref plus canonical review state. GitHub may project it as a pull request while another forge may project the same canonical object differently.

## Wiki

Canonical wiki content lives under `.project/wiki/` as ordinary Markdown and assets. The GitHub adapter will project that content into the repository's GitHub Wiki Git repository or another supported GitHub-facing representation.

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

### Incoming event envelope

Planned normalized event representation:

```text
incoming/2026/09/<event-id>/
  event.toml
  body.md       # when the event has prose
  raw.json      # optional provider payload
```

Example `event.toml`:

```toml
schema = "allodium.incoming-event/v0"
id = "github-event-01K..."
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
remote_event_id = "98214321"
created_at = "2026-09-16T11:59:55Z"
url = "https://github.com/owner/repository/issues/37#issuecomment-98214321"
```

Provider identity remains provider-scoped. Matching usernames on two forges are not assumed to denote the same person.

## Canonical versus reconstructible

A file being checked into the repository does not automatically make it canonical. Allodium documents the semantic class of each subtree:

- canonical: project-owned meaning;
- configuration: project-owned instructions for projection;
- incoming: durable foreign-originated provenance;
- reconstructible: mappings/observations/caches that can be rebuilt.

All of them remain ordinary user-editable files. The distinction governs reconciliation semantics, not filesystem permissions.
