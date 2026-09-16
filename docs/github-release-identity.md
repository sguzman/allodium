# GitHub Release identity

GitHub Releases are projections around canonical `.project/releases/` records. For the GitHub v0 adapter, the immutable provider identity anchor is the mapped Git ref for the canonical tag, resolved to the canonical full commit SHA. The Release object's `tag_name` is provider attachment metadata, not an independent source of identity.

Live dogfood demonstrated why this distinction is necessary: mutating a draft Release's title caused GitHub to rewrite the Release object's `tag_name` to an internal `untagged-*` slug while leaving the real Git tag intact. Allodium archived that drift before repair, then restored the canonical attachment.

## Rules

- Canonical Allodium ID owns release identity.
- Canonical `revision` owns release commit intent.
- Canonical `tag` names the GitHub projection tag.
- The mapped Git ref `refs/tags/<tag>` must resolve to the canonical revision and is immutable in projection v0.
- Release `tag_name` is observed provider attachment state.
- Mutable Release PATCH operations reassert canonical `tag_name` and `target_commitish`.
- Provider attachment drift is archived as incoming evidence before reconstructible observation advances.
- A moved or conflicting Git tag is immutable identity drift and is never silently retargeted.
