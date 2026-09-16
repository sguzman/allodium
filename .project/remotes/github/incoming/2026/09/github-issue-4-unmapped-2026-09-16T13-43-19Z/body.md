This issue was deliberately created directly on GitHub, outside Allodium canonical state, to test how Allodium handles a remote issue that has no canonical mapping.

It MUST NOT silently become a canonical `.project/issues/*` object. The desired behavior is provider-scoped durable ingress with provenance, followed by an explicit adoption/promotion decision if we ever want it canonical.

This is test evidence, not canonical project authority.

Revision 2 dogfood: this sentence was added directly on GitHub to prove Allodium appends a new unmapped provider revision instead of rewriting the first capture.