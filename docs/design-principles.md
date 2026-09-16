# Design principles

## 1. The user is sovereign

The user must be able to inspect, edit, copy, move, delete, repair, and script against authoritative project state using ordinary filesystem tools.

Allodium exists to prevent a project-management service, forge, VCS implementation detail, or application database from becoming the only meaningful owner of operational project data.

## 2. Filesystem-native, not merely Git-native

A Git object or special ref is more portable than forge-owned state, but it is still below the line Allodium draws. Canonical Allodium state MUST be represented as ordinary files and directories in the working tree.

Git is a versioning and transport mechanism for those files. It is not Allodium's database.

## 3. Tool death must be survivable

If every Allodium binary stops working, a person must still be able to understand the project state from the directory tree and documented formats.

Accordingly, canonical state SHOULD favor:

- TOML for structured metadata;
- Markdown for human-authored prose;
- one object per directory when objects have multiple components;
- stable, human-visible identifiers;
- explicit schemas and version markers.

Canonical state MUST NOT require hidden refs, SQLite, opaque binary blobs, or a proprietary service.

## 4. Projection is downstream

A forge projection consumes canonical state and attempts to make the remote service reflect it.

The remote service does not silently become canonical merely because a collaborator used its UI.

## 5. Inbound activity is preserved, not erased

External activity is legitimate project history. Allodium captures it locally with provenance while keeping it namespaced to the authority where it occurred.

A GitHub comment remains a GitHub-originated event unless explicitly promoted into canonical project state.

## 6. Remote authorities are isolated

GitHub-originated state does not automatically become GitLab-originated state, and vice versa. Remote social surfaces can surround the same canonical artifact without pretending their users, identifiers, reactions, or conversations are globally identical.

## 7. Promotion is explicit

Remote activity MAY be promoted into canonical project state, but promotion must be explicit and provenance-preserving.

Projection may consume canonical state. Remote activity may propose canonical changes. Remote activity may never silently redefine canonical state.

## 8. Derived state is disposable

Mappings, observed snapshots, caches, and other projection bookkeeping MAY live in the filesystem, because user sovereignty includes derived state too. But they must be marked reconstructible and must never be required to recover canonical meaning.

## 9. GitHub first, without GitHub ownership

The first complete adapter targets GitHub. Allodium should model GitHub seriously rather than pretend all forges are identical. Portability comes from the canonical model and adapter boundary, not from reducing GitHub to the lowest common denominator.

## 10. Boring representation is a feature

A clever internal representation that weakens manual intelligibility is a regression, even if it improves implementation convenience.
