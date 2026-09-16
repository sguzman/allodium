# GitHub runtime boundary

The GitHub adapter is deliberately separate from `allodium-core`.

`allodium-core` understands the sovereign filesystem model and can validate and plan with no network access. `allodium-github` is the projection runtime: it knows GitHub's REST API, authentication environment, remote revision tokens, and the mechanics of observing and applying GitHub state.

## Commands

```text
allodium github observe [ROOT] [REMOTE]
allodium github plan    [ROOT] [REMOTE]
allodium github apply   PLAN [ROOT]
```

The phases are intentionally separate.

1. `observe` reads GitHub and writes reconstructible observations plus durable incoming provenance.
2. `plan` is pure filesystem logic and emits a TOML plan without network access.
3. `apply` consumes an explicit plan file and performs only the operations represented there.

This separation is part of the sovereignty model. A GitHub response can become evidence in `observed/` or `incoming/`; it cannot quietly redefine canonical project state while a plan is being formed.

## Authentication

Read-only observation of public repositories can run without credentials. Mutation requires a token from the process environment. Resolution order is:

1. `ALLODIUM_GITHUB_TOKEN`
2. `GH_TOKEN`
3. `GITHUB_TOKEN`

Allodium does not write the token into `.project/`, Git configuration, a credential file, a generated plan, or remote observation state.

The narrowest practical GitHub token permissions should be used. For the M1 issue slice, write operations need repository Issues write access. Repository contents permission is not intrinsically required by the adapter merely to mutate GitHub Issues.

## Optimistic remote revision guard

Observed issues have a reconstructible sibling revision file:

```text
.project/remotes/github/observed/issues/issue-0001.revision.toml
```

It records GitHub's `updated_at` for the issue revision that Allodium actually observed.

Before an `update_issue` plan operation is applied, the adapter fetches the issue again. If GitHub's current `updated_at` no longer matches the locally observed revision, Allodium refuses the mutation and requires another observe/plan cycle.

This is intentional. Canonical filesystem authority does not mean permission to trample newer remote collaboration without noticing it.

## Managed-field external changes

When `observe` sees GitHub title/body/state differ from the previous local observation, it first archives the transition under `incoming/` before replacing reconstructible `observed/` state.

The archive contains:

```text
event.toml
before.toml
before.body.md
after.toml
after.body.md
```

GitHub's normal issue representation does not identify the actor responsible for the latest edit. Allodium therefore records the remote `updated_at`, the changed fields, and the fact that the actor is unknown. It does not guess from the issue creator or from repository ownership.

## Comment ingress

Issue comments are fetched during observation. The first observed revision becomes an `issue.comment.created` incoming event and a reconstructible observed-comment snapshot. If a later GitHub `updated_at` or body differs, Allodium creates an `issue.comment.edited` event and advances only the reconstructible observation.

Remote comments never become canonical issue prose automatically.
