# Vocabulary

**allodial project state** — project state held directly by the user as ordinary filesystem objects rather than held only through a forge, database, or VCS-specific hidden representation.

**canonical state** — what the project itself asserts as authoritative.

**projection** — a representation of canonical project state on an external service such as GitHub.

**remote** — a named external authority/service instance to which state is projected or from which activity is observed.

**observed state** — reconstructible local description of what a remote most recently appeared to contain.

**mapping** — reconstructible relationship between an Allodium identity and a remote identity.

**incoming event** — locally preserved evidence of externally originated remote activity.

**promotion** — explicit adoption of remote-originated information into canonical project state while retaining provenance.

**review** — canonical Allodium concept for a proposed code/state change that may project to a GitHub Pull Request, GitLab Merge Request, or equivalent.

**reconciliation** — comparison of canonical desired state with observed remote state followed by policy-governed actions.
