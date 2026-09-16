# Architecture

Allodium separates project state into four classes.

```text
                         +------------------+
                         | canonical state  |
                         | .project/*       |
                         +--------+---------+
                                  |
                            desired state
                                  |
                 +----------------+----------------+
                 |                                 |
                 v                                 v
        +----------------+                +----------------+
        | GitHub adapter |                | future adapter |
        +-------+--------+                +----------------+
                |
                v
        +----------------+
        | GitHub service |
        +-------+--------+
                |
       incoming | observed
                v
  .project/remotes/github/
```

## Canonical state

Canonical state describes what the project itself asserts: issues, reviews, wiki content, milestones, decisions, releases, and other durable project concepts.

Canonical objects use Allodium IDs such as `issue-0001`. A GitHub number such as `#37` is never the object's canonical identity.

## Projection configuration

`.project/remotes/<name>/remote.toml` says where and how a projection operates. This configuration is ordinary project-owned state even though it describes a foreign system.

## Observed state

`observed/` is a reconstructible description of what the adapter most recently observed remotely. It exists to support reconciliation, diagnostics, and conflict reporting.

It is explicitly non-canonical.

## Mappings

`mappings/` records relationships such as:

```text
issue-0001 -> github issue #37
review-0004 -> github pull request #12
```

Mappings are useful but reconstructible. Deleting them may make the next reconciliation expensive or require rediscovery, but it must not destroy canonical project meaning.

## Incoming provenance archive

`incoming/` records externally originated activity. It is append-oriented: create, edit, delete, close, reopen, review, reaction, label mutation, and similar events are distinct historical observations where the upstream API permits that distinction.

An incoming event should contain normalized metadata and may retain a raw provider payload beside it when configured.

## Promotion

Promotion copies or transforms meaningful remote-originated information into a canonical object while retaining an origin link. Promotion is an explicit act, not an automatic side effect of synchronization.

## Reconciliation

For a remote `github`, reconciliation is conceptually:

1. Load and validate canonical `.project/` state.
2. Load GitHub projection configuration and mappings.
3. Observe relevant GitHub state.
4. Capture previously unseen external activity into `incoming/`.
5. Calculate desired-versus-observed differences.
6. Apply permitted outbound changes.
7. Refresh reconstructible `observed/` and `mappings/` state.
8. Report conflicts rather than silently allowing foreign state to redefine canonical state.

## Implementation layers

- `allodium-core`: schemas, loading, validation, identity, normalization.
- `allodium` CLI: human and automation entry point.
- adapter modules: GitHub first; later GitLab, Forgejo, etc.
- `.github/workflows/*`: intentionally thin GitHub execution wrappers.

The GitHub workflow should eventually do little more than checkout, invoke Allodium, and commit accepted ingress/projection bookkeeping changes when policy permits.
