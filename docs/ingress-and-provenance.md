# Ingress and provenance

Remote collaboration must survive the remote.

Allodium therefore treats externally originated activity as durable local history rather than transient forge UI state.

## Append-oriented archive

Where provider evidence permits, an externally created object and its later mutations should become separate historical observations rather than a single mutable local mirror.

For example:

```text
comment created -> event A
comment edited  -> event B
comment deleted -> event C
```

This lets the repository preserve history that a provider's current UI may no longer show.

## Deterministic event identity

When the provider supplies a stable object/event identifier, Allodium derives a deterministic event directory name from the remote namespace, object kind, provider identifier, and event kind. Re-ingesting the same evidence must therefore be idempotent.

If an event directory already exists with byte-identical normalized metadata and body, ingestion is a no-op. If the same derived identity points at different evidence, Allodium reports a collision rather than silently rewriting history.

## Evidence discipline

Allodium must distinguish:

- facts directly returned by a provider API;
- normalized interpretations of those facts;
- inferred relationships;
- missing history the provider does not expose.

The system must not fabricate an edit event merely because two polls returned different text. It may record an observation change with timestamps and explicitly state that the exact mutation time is unknown.

Missing provider fields remain missing. For example, if the adapter receives a comment but the integration surface does not expose its provider `created_at`, the normalized event omits `source.created_at` and records the limitation in `evidence`; it does not substitute the local capture time and pretend that was the provider creation time.

## Provider-scoped identity

An actor is initially identified within a remote authority:

```text
github:user:284928
gitlab:user:5521
```

Equal usernames do not establish cross-provider identity.

A future explicit identity-link feature may assert correspondences, but remote ingestion must not guess them.

## Raw payload retention

Adapters may optionally retain a `raw.json` payload beside a normalized event. Raw payloads are ordinary files too, but can contain volatile or unnecessary provider data. Retention should therefore be configurable and documented.

Normalized event metadata must be sufficient for normal human inspection even when raw retention is disabled.

## First dogfood event

Allodium's repository deliberately created a real comment on GitHub Issue #2 and archived it under:

```text
.project/remotes/github/incoming/2026/09/github-issue-comment-5697573876-created/
  event.toml
  body.md
```

The comment remains GitHub-local social state. Its local archive is provenance, not canonical issue prose. This is the first concrete demonstration of remote activity entering the sovereign filesystem without acquiring authority over canonical state.

## Promotion

Promotion means deliberately adopting some remote-originated information into canonical project state.

A promoted artifact should record at least:

- remote name;
- provider object/event identifier;
- source URL when stable and available;
- external actor attribution;
- capture time;
- transformation note when canonical wording differs materially from the source.

Promotion does not erase or rewrite the original incoming event.
