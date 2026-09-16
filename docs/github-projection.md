# GitHub projection

GitHub is Allodium's first implementation target and should be supported deeply.

The objective is not to avoid GitHub. The objective is to make GitHub replaceable without making GitHub useless.

## Direction of authority

```text
canonical .project state  --->  GitHub managed state
GitHub external activity  --->  namespaced incoming archive
```

This is deliberately not naïve two-way synchronization.

## Initial projection surface

The GitHub adapter is intended to cover, in roughly this order:

1. Issues: title, body, open/closed state, labels, milestones, assignees where representable.
2. Issue comments and relevant issue timeline activity as inbound provenance.
3. Reviews projected as pull requests.
4. Pull request conversation, review submissions, and inline review threads as inbound provenance.
5. Wiki pages and assets.
6. Releases and release notes.
7. Discussions where API support and semantics are adequate.
8. Project/board concepts after the canonical model is mature enough not to inherit GitHub's ontology accidentally.

## Identity

Allodium identity wins:

```text
issue-0001 -> GitHub #37
```

The `#37` mapping is projection state, not the issue's identity.

The same issue may later map to another remote without changing canonical identity.

## External edits

If a collaborator edits an Allodium-managed GitHub issue directly, the adapter must not quietly treat the edit as canonical truth.

Instead it should:

1. observe the difference;
2. archive the remote-originated event/delta with provenance where possible;
3. report a reconciliation conflict or pending inbound proposal;
4. apply policy explicitly.

Some provider APIs expose only current state rather than a complete mutation history. Allodium must record that limitation rather than invent provenance it does not possess.

## External comments

Comments are naturally remote-social state. They should be ingested into the GitHub namespace and preserved. They are not automatically re-emitted onto another forge under the original author's identity.

A project maintainer may explicitly promote a remote comment or finding into canonical state. Promotion retains an origin record.

## Workflow

`.github/workflows/` is permitted and expected. It is GitHub-specific execution glue, not canonical project state.

The intended mature workflow is approximately:

```text
checkout
allodium validate
allodium github ingest
allodium github plan
allodium github apply
commit newly captured ingress/mappings if policy permits
```

The meaningful behavior belongs in Allodium code, not in a giant Actions YAML program.

## No lowest-common-denominator trap

GitHub-specific capabilities may have GitHub-specific adapter state beneath `.project/remotes/github/`. The canonical model should remain forge-independent where the project concept is genuinely universal, while remote-specific facts stay namespaced instead of contaminating canonical ontology.
