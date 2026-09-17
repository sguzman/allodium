Implement the first canonical label-definition projection vertical slice without making GitHub label names the owner of label identity.

## Acceptance criteria

- [x] Canonical `.project/labels/*.toml` records load and validate without GitHub knowledge.
- [x] The label `id` is project-owned and must match the filename stem; `name` is mutable display text, not identity.
- [x] Canonical names are non-empty; optional descriptions are preserved; optional colors validate as `#RRGGBB`.
- [x] GitHub adapter planning is deterministic and network-free.
- [x] Stable GitHub mappings use provider identity rather than treating mutable label names as canonical identity.
- [x] Observed label snapshots preserve provider ID, name, color, description, and other relevant provider metadata as reconstructible state.
- [x] GitHub label create and mutable update/rename reconcile outward through the normal inspectable plan/apply path.
- [x] Because GitHub labels expose no `updated_at` revision, update safety re-lists the provider label by stable provider ID and requires exact equality with the last observed managed snapshot before PATCH.
- [x] Provider-side managed label changes are archived as GitHub-scoped incoming evidence before reconstructible observation advances.
- [x] A provider rename is resolved through stable provider identity rather than losing the mapping because the old label name disappeared.
- [x] GitHub adapter rejects provider-name collisions rather than silently adopting an unmapped foreign label by display name.
- [x] A real canonical `label-allodium` was projected to GitHub, deliberately drifted on the provider, repaired from canonical state, and re-observed idempotently.
- [x] Issue↔label membership remains explicitly out of scope until Issue schema semantics reference canonical label IDs; this slice manages repository label definitions only.
- [x] Canonical label-file absence does not delete a GitHub label in v0. GitHub label deletion can erase provider issue associations that Allodium does not yet canonically own, so destructive deletion semantics require a later explicit design.

## Completion evidence

- Canonical dogfood object: `.project/labels/label-allodium.toml`.
- Durable GitHub mapping: canonical `label-allodium` → provider label ID `12237280577`.
- Live creation converged to an idempotent plan with no label operation.
- A provider-only rename changed `allodium` to `provider-drift-m4-label-dogfood` while retaining provider ID `12237280577`.
- Reconciliation archived exactly one managed-field drift event with `fields = ["name"]`, then planned `update_label` against the stable provider ID and restored canonical `allodium`.
- Durable drift evidence lives under `.project/remotes/github/incoming/2026/09/github-label-12237280577-managed-change-f728f3dcb8a5d885/`.
- The following clean reconciliation archived zero new label drift, planned no label operation, applied zero label mutations, and produced no durable GitHub state change.
- The temporary provider-drift workflow was removed; the permanent workflow surface returned to the normal CI + Allodium sync pair.

The dogfood object for this slice is `.project/labels/label-allodium.toml`. This issue is canonical Allodium state; any GitHub Issue projection is downstream.

---

**Allodium canonical ID:** `issue-0009`

This GitHub issue is a projection of `.project/issues/issue-0009/`, not the canonical record.