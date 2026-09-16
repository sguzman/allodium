Implement the first wiki projection vertical slice without making GitHub Wiki the canonical documentation store.

## Acceptance criteria

- Canonical `.project/wiki/wiki.toml` loads and validates without GitHub knowledge.
- The declared canonical home page is a safe relative path under `.project/wiki/` and must exist as an ordinary file.
- Canonical wiki pages remain ordinary user-editable files; the directory is the page inventory rather than a provider-generated manifest.
- GitHub Wiki state is treated as a projection into GitHub's separate wiki Git repository, never as canonical state.
- Projection planning is inspectable before mutation and does not require network access.
- Observed GitHub Wiki revision identity is persisted under `.project/remotes/github/observed/wiki/` so outbound writes can be guarded against stale remote state.
- Applying a wiki plan performs no mutation after a stale observed revision is detected.
- Provider-only wiki changes are preserved as GitHub-scoped incoming evidence rather than silently promoted into `.project/wiki/`.
- Wiki assets have an explicit v0 policy; unsupported asset/path cases fail loudly rather than being silently rewritten.
- At least one real canonical Allodium wiki page is projected to the enabled GitHub Wiki and re-observed idempotently.

This issue is canonical Allodium state. The GitHub Wiki repository is a transport/projection surface around `.project/wiki/`, not the owner of the wiki.
