# Allodium

**The filesystem is the authority. Forges are projections.**

Allodium is a forge-independent project-state substrate built around user sovereignty. Issues, reviews, wiki pages, milestones, discussions, decisions, release metadata, and remote interaction history should exist as ordinary files that a person can inspect and edit with ordinary filesystem tools.

Git is useful for versioning and transport, but Git does not own the data model. GitHub is useful for hosting and collaboration, but GitHub does not own project state.

```text
USER
  |
  v
ordinary files under .project/
  |
  +--> Git history / transport
  |
  +--> GitHub projection
  |
  +--> future GitLab / Forgejo / other projections
```

## Core invariant

> Tooling may disappear. The project must remain intelligible and editable.

Authoritative state therefore lives in plain directories, TOML, Markdown, and other deliberately boring files. No hidden Git refs, required database, proprietary export, or forge-owned identifier is allowed to become the canonical representation.

## GitHub is the first projection

Allodium is intentionally being developed with GitHub in mind first. That is not an attempt to hide GitHub behind a prematurely generic abstraction. GitHub is the first real projection target and will drive the first complete implementation of issues, pull-request-shaped reviews, wiki projection, releases, and inbound collaboration capture.

The architecture nevertheless keeps GitHub downstream of `.project/`. A future GitLab or Forgejo adapter should consume the same canonical state rather than introduce a second source of truth.

## Repository layout

```text
.project/
  manifest.toml             canonical project identity
  issues/                   canonical issue state
  reviews/                  canonical review/change state
  wiki/                     canonical wiki content
  milestones/               canonical milestones
  discussions/              canonical discussions
  releases/                 canonical release metadata
  remotes/
    github/
      remote.toml           GitHub projection configuration
      mappings/             reconstructible projection identity mappings
      incoming/             append-oriented inbound provenance archive
      observed/             reconstructible current remote observations

docs/                       Allodium's own design documentation
crates/                     implementation
.github/                    thin GitHub-specific execution glue
```

## Current status

M0 established the constitutional filesystem substrate. M1 is implementing the first GitHub issue reconciliation vertical slice.

```bash
cargo run -p allodium-cli -- validate .
cargo run -p allodium-cli -- github plan .
```

`github plan` emits a serializable dry-run document. It compares canonical issue files against checked-in GitHub mappings and observations without making network calls or silently adopting remote state.

See [`docs/design-principles.md`](docs/design-principles.md), [`docs/format.md`](docs/format.md), [`docs/github-projection.md`](docs/github-projection.md), and [`docs/roadmap.md`](docs/roadmap.md).
