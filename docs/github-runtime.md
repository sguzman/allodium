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

The narrowest practical GitHub token permissions should be used. Issue projection needs repository Issues write access; review and release projection use their corresponding GitHub repository capabilities. Wiki projection uses Git transport only after GitHub has initialized its separate wiki repository.

## Optimistic remote revision guard

Observed issues have a reconstructible sibling revision file:

```text
.project/remotes/github/observed/issues/issue-0001.revision.toml
```

It records GitHub's `updated_at` for the issue revision that Allodium actually observed.

Before an `update_issue` plan operation is applied, the adapter fetches the issue again. If GitHub's current `updated_at` no longer matches the locally observed revision, Allodium refuses the mutation and requires another observe/plan cycle.

Other provider objects use the strongest reconstructible revision identity available to that surface. Reviews compare the observed pull-request update timestamp plus base/head commit identities. Wiki projection compares the observed wiki branch and Git HEAD. Releases compare the complete managed observed release snapshot plus the commit to which the release tag resolves.

This is intentional. Canonical filesystem authority does not mean permission to trample newer remote collaboration without noticing it.

## Managed-field external changes

When `observe` sees GitHub managed fields differ from the previous local observation, it archives the transition under `incoming/` before replacing reconstructible `observed/` state.

The exact before/after evidence is surface-specific, but the invariant is the same: provider changes become local evidence, not implicit canonical edits.

GitHub's normal issue representation does not identify the actor responsible for the latest edit. Allodium therefore records the remote revision information and the fact that the actor is unknown when the provider cannot prove an actor. It does not guess from the object creator or repository ownership.

## Comment ingress

Issue comments are fetched during observation. The first observed revision becomes an `issue.comment.created` incoming event and a reconstructible observed-comment snapshot. If a later GitHub `updated_at` or body differs, Allodium creates an `issue.comment.edited` event and advances only the reconstructible observation.

Remote comments never become canonical issue prose automatically.

## GitHub Wiki bootstrap boundary

GitHub Wiki is unusual because the repository's Wiki setting and the separate wiki Git repository are not the same capability surface.

GitHub documents that the wiki repository can be cloned only after an initial page has been created on GitHub. Before that bootstrap occurs, `OWNER/REPOSITORY.wiki.git` is not an initialized remote and the corresponding repository resource is absent. In live Allodium dogfood, GitHub returned `404` for `sguzman/allodium.wiki` and rejected the initial Git push even though the main repository reported Wiki as enabled.

Allodium treats that as a provider bootstrap prerequisite, not as permission to move canonical wiki ownership into GitHub. The runtime therefore:

- may observe that Wiki is enabled but uninitialized;
- may plan the desired canonical wiki tree;
- refuses to pretend projection succeeded;
- leaves `.project/wiki/` untouched;
- emits an explicit bootstrap-boundary diagnostic;
- does not use undocumented endpoints to fabricate the provider's hidden wiki repository.

Once a first provider page exists, normal Allodium Git transport can observe the wiki HEAD, reconcile canonical Markdown pages, and guard writes against stale remote commits.
