Implement the first canonical label-definition projection vertical slice without making GitHub label names the owner of label identity.

## Acceptance criteria

- Canonical `.project/labels/*.toml` records load and validate without GitHub knowledge.
- The label `id` is project-owned and must match the filename stem; `name` is mutable display text, not identity.
- Canonical names are non-empty; optional descriptions are preserved; optional colors validate as `#RRGGBB`.
- GitHub adapter planning is deterministic and network-free.
- Stable GitHub mappings use provider identity rather than treating mutable label names as canonical identity.
- Observed label snapshots preserve provider ID, name, color, description, and other relevant provider metadata as reconstructible state.
- GitHub label create and mutable update/rename reconcile outward through the normal inspectable plan/apply path.
- Because GitHub labels expose no `updated_at` revision, update safety re-lists the provider label by stable provider ID and requires exact equality with the last observed managed snapshot before PATCH.
- Provider-side managed label changes are archived as GitHub-scoped incoming evidence before reconstructible observation advances.
- A provider rename must be resolved through stable provider identity rather than losing the mapping because the old label name disappeared.
- GitHub adapter rejects provider-name collisions rather than silently adopting an unmapped foreign label by display name.
- A real canonical `label-allodium` is projected to GitHub, deliberately drifted on the provider, repaired from canonical state, and re-observed idempotently.
- Issue↔label membership remains out of scope until Issue schema semantics reference canonical label IDs; this slice manages repository label definitions only.
- Canonical label-file absence does not delete a GitHub label in v0. GitHub label deletion can erase provider issue associations that Allodium does not yet canonically own, so destructive deletion semantics require a later explicit design.

The dogfood object for this slice is `.project/labels/label-allodium.toml`. This issue is canonical Allodium state; any GitHub Issue projection is downstream.
