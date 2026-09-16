# Ingress and provenance

Remote collaboration must survive the remote.

Allodium therefore treats externally originated activity as durable local history rather than transient forge UI state.

## Append-oriented archive

Where provider evidence permits, an externally created object and its later mutations should become separate historical observations rather than a single mutable local mirror.

For example:

```text
comment created -> event A
comment edited  -> event B
comment no longer observed -> event C
```

This lets the repository preserve history that a provider's current UI may no longer show without claiming facts the provider surface cannot prove.

## Deterministic event identity

When the provider supplies a stable object/event identifier, Allodium derives a deterministic event directory name from the remote namespace, object kind, provider identifier, and event kind. Re-ingesting the same evidence must therefore be idempotent.

If an event directory already exists with byte-identical normalized metadata and body, ingestion is a no-op. If the same derived identity points at different evidence, Allodium reports a collision rather than silently rewriting history.

For GitHub issues that have no canonical Allodium mapping, the provider issue number plus GitHub's `updated_at` identifies one observed provider revision. The event directory is bucketed by that provider revision timestamp, not by the local poll time. Re-observing the same remote revision on a later day therefore resolves to the same event instead of manufacturing new history.

## Evidence discipline

Allodium must distinguish:

- facts directly returned by a provider API;
- normalized interpretations of those facts;
- inferred relationships;
- missing history the provider does not expose.

The system must not fabricate an edit event merely because two polls returned different text. It may record an observation change with timestamps and explicitly state that the exact mutation time is unknown.

Representation-only differences are not project history. For example, a trailing newline added or omitted by a serialization boundary must not become a claimed remote body edit when the durable text is otherwise equivalent.

Missing provider fields remain missing. For example, if the adapter receives a comment but the integration surface does not expose its provider `created_at`, the normalized event omits `source.created_at` and records the limitation in `evidence`; it does not substitute the local capture time and pretend that was the provider creation time.

## Comments no longer observed

A comment that was previously observed and later disappears from GitHub's current issue-comment listing is evidence of **absence from that observation surface**, not proof of a particular deletion event.

Allodium therefore archives an append-only event with kind:

```text
issue.comment.no_longer_observed
```

The event preserves the last-known comment body, provider object ID, URL, last-known provider `updated_at`, and last-known actor identity. Its evidence text explicitly states that no deletion actor or exact deletion timestamp is asserted.

The deterministic event identity is derived from the comment ID and its last-known provider revision. Repeated polls while the same comment remains absent therefore do not create repeated history.

If a provider later supplies a stronger deletion event or audit record, that stronger evidence can be archived separately. It must not retroactively rewrite the weaker earlier observation.

## Unmapped remote issues

A forge can contain issues that were created outside Allodium and therefore have no canonical `.project/issues/<id>/` object and no canonical-to-remote mapping. Discovery of such an issue is **ingress, not promotion**.

For GitHub, Allodium lists repository issues in addition to observing its mapped projections. A GitHub issue that is not a pull request and whose number is absent from the mapping table is archived under the GitHub remote namespace as provider-scoped evidence:

```text
.project/remotes/github/incoming/YYYY/MM/
  github-issue-<number>-unmapped-<provider-updated-at>/
    event.toml
    body.md
```

The event records the provider issue number, URL, title, state, actor identity exposed by GitHub, provider creation/update timestamps, local first-observation time, and an explicit evidence statement that no canonical mapping existed.

This operation MUST NOT:

- create `.project/issues/*`;
- create or modify a canonical-to-GitHub mapping;
- treat remote title/body/state as canonical project state;
- infer that a remote author intended adoption into Allodium.

A later GitHub revision with a different provider `updated_at` becomes another append-only event. Re-observing an already archived revision is a no-op. If the same deterministic revision identity points to different durable evidence, Allodium reports a collision rather than rewriting the first capture.

Adoption of an unmapped remote issue requires an explicit future promotion/adoption operation.

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

## Dogfood evidence

Allodium's repository deliberately created a real comment on GitHub Issue #2 and archived it under:

```text
.project/remotes/github/incoming/2026/09/github-issue-comment-5697573876-created/
  event.toml
  body.md
```

The comment remains GitHub-local social state. Its local archive is provenance, not canonical issue prose. This was the first concrete demonstration of remote activity entering the sovereign filesystem without acquiring authority over canonical state.

The repository then deliberately created GitHub Issue #4 directly on GitHub with no canonical Allodium issue. The first observation was archived as:

```text
.project/remotes/github/incoming/2026/09/
  github-issue-4-unmapped-2026-09-16T13-34-44Z/
    event.toml
    body.md
```

After that capture, the mapping table still contained only canonical issues 1 through 3 and `.project/issues/` still contained only `issue-0001` through `issue-0003`. This is the dogfood proof that observing a forge-native issue does not grant it canonical authority.

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
