Harden GitHub event-driven reconciliation so provider events wake the right Allodium observations without making webhook payloads, GitHub Actions context, or delivery timing authoritative project state.

## Constitutional rule

A provider event is a wake-up hint, not evidence of canonical truth.

The normal path remains:

```text
provider event
    |
    v
wake serialized reconciliation
    |
    v
fetch current provider state
    |
    +--> archive provider-scoped evidence
    +--> compare against canonical files
    +--> derive inspectable plan
    +--> apply only authorized canonical projection
```

Allodium must therefore remain correct if an event is duplicated, delayed, reordered, omitted, or contains fields the adapter does not trust. The event may decide *when* to look, but observation decides *what is currently true*.

## Provider audit

The permanent GitHub workflow already wakes on successful CI for `main`, selected Issue/Milestone/Label events, Issue comments, pull-request root/review/inline-comment events, Discussion root/comment events, and manual dispatch. Every run checks out the default branch and executes the same full observe → plan → apply → re-observe → validate loop.

GitHub currently exposes more activity types than Allodium subscribes to. That is not by itself a reason to add them. A trigger is useful only when the affected provider state is represented by an Allodium observation/archive surface or when the event is required to notice a safe disappearance boundary.

GitHub Discussion workflow events remain public preview. ProjectV2 webhook events are a separate provider surface: current GitHub documentation exposes `projects_v2` and `projects_v2_item` as organization-level webhook events requiring Projects organization permission, rather than as repository Actions events suitable for Allodium's currently configured user-owned Project target. This issue must not pretend repository event coverage exists where GitHub does not provide it.

## Acceptance criteria

- [ ] Document an explicit event-to-observation matrix for every permanent GitHub workflow trigger.
- [ ] Treat event payloads only as wake-up/context hints; never write canonical or provider-observation state directly from an Actions payload when the provider API can be re-fetched.
- [ ] Audit the existing trigger list against state Allodium actually observes; remove irrelevant wakeups and add materially missing wakeups only where observation semantics are defined.
- [ ] For every subscribed provider deletion/disappearance activity, define a non-destructive observation outcome before relying on the trigger. Missing provider state must preserve evidence and must not fabricate a deletion actor, deletion timestamp, or canonical deletion.
- [ ] Make duplicate/reordered wakeups harmless through serialized reconciliation and idempotent observation/archive behavior.
- [ ] Keep `pull_request_target` execution safe: checkout trusted default-branch code only, never execute contributor-controlled PR contents, and document why write credentials are not exposed to untrusted code.
- [ ] Preserve the successful-main `workflow_run` bridge so Actions-authored product/state commits cannot silently strand canonical changes when `GITHUB_TOKEN` suppresses recursive push-triggered workflows.
- [ ] Document the GitHub Projects event boundary separately from repository events; do not install or assume organization webhooks as a side effect of ordinary repository synchronization.
- [ ] Add executable/static tests or validation that fail when the permanent workflow trigger contract drifts from the documented supported matrix.
- [ ] Dogfood at least one externally originated provider edit and one social/comment activity through event-triggered reconciliation, then prove an identical follow-up observation is a no-op.
- [ ] Update the M4 roadmap item only after the event-driven contract is executable and dogfooded.

## Initial trigger review

The current Issue root observation owns managed title/state plus exact body observation used by reconciliation; canonical issue labels exist as project metadata but provider issue-label membership is not yet a canonical association contract. Review root observation owns title/state/base/head revisions and merge state, while review conversation, review submissions, inline comments, and inline threads have their own provider-scoped social archives. Milestones, labels, Discussions, releases, Wiki, and Projects each have separate provider observations and capability boundaries.

That means the correct trigger set is not "all GitHub activity types." For example, adding an event for a provider field Allodium neither observes nor archives only spends CI and creates the illusion of support. Conversely, subscribing to `deleted` without teaching the observer how to preserve disappearance safely can turn a useful wake-up into a reconciliation failure.

The first implementation slice should therefore build the trigger/evidence matrix and test it against the checked-in workflow, then harden any already-subscribed destructive events before expanding coverage.

## Non-goals

- Treating webhook payloads as canonical state.
- Replacing periodic/manual reconciliation with an assumption that provider events are perfectly delivered.
- Installing organization-level GitHub Projects webhooks automatically.
- Cross-forge replay of GitHub user actions.
- Adding triggers for provider activity Allodium intentionally does not model.

This issue is canonical Allodium state. Any GitHub Issue projection of this work item is downstream.

---

**Allodium canonical ID:** `issue-0012`

This GitHub issue is a projection of `.project/issues/issue-0012/`, not the canonical record.