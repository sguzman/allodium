# Format versioning and migration policy

Allodium's filesystem is a public storage contract. A format change is therefore not an implementation detail: it changes files that users own and may edit without Allodium installed.

## Schema identity

Every structured record declares an explicit schema identifier:

```text
allodium.<record-kind>/v<N>
```

Examples:

```text
allodium.project/v0
allodium.issue/v0
allodium.review/v0
allodium.github.observed-revision/v0
```

Versions belong to record kinds independently. Changing the issue schema does not require changing the project-manifest schema.

Provider-specific records include the provider namespace because their semantics are not canonical Allodium semantics.

## Reader rule: fail closed on unknown semantics

A reader MUST reject a schema version whose semantics it does not implement. It must not silently reinterpret a newer record as an older one.

This applies especially to canonical and incoming provenance records. A validator may report an unsupported schema, but must not rewrite it merely by reading it.

Unknown optional fields inside a supported schema may be tolerated when they do not alter the meaning of known fields. Tolerance is a compatibility mechanism, not permission for the writer to change semantics without a version change.

## What requires a new schema version

A record kind must advance from `vN` to `vN+1` when a change can alter the meaning of an existing valid record, including:

- removing or renaming a field;
- changing a field's type;
- changing the interpretation of an existing value;
- changing identity rules;
- changing required directory relationships;
- making a previously optional field required without a deterministic default;
- changing authority or provenance semantics.

A version bump is normally unnecessary for a new optional field whose absence preserves the old meaning.

## v0 does not mean unversioned

`v0` is experimental, but it is still an explicit contract. Development may replace `v0` with `v1`, but tooling must not reinterpret already-written `v0` files in place without an explicit migration.

Before Allodium declares a record kind `v1`, its identity rules, authority class, required fields, and migration behavior should be stable enough that ordinary repositories can depend on them.

## Migrations are explicit writes

Reading, validating, observing, planning, or projecting MUST NOT perform an implicit canonical schema migration.

A migration is a separate user-visible filesystem mutation. A future migration command should behave conceptually like:

```text
allodium migrate --to <schema-version>
```

A migration must:

1. identify every file it intends to change;
2. refuse unsupported or ambiguous source versions;
3. produce deterministic output for deterministic input;
4. preserve canonical IDs unless the migration explicitly documents an identity change;
5. preserve incoming provenance rather than rewriting historical evidence to look newly observed;
6. leave provider mappings reconstructible rather than treating them as canonical identity;
7. be reviewable as an ordinary filesystem/Git diff.

If a migration cannot preserve meaning automatically, it must stop and require an explicit human decision rather than inventing one.

## No hidden migration ledger

The authoritative result of a migration is the migrated ordinary files themselves. Allodium must not require a hidden database, Git note, custom ref, or provider record to know which schema a file uses; the schema identifier is inside the file.

A migration tool may use temporary working state while it runs, but that state is disposable and non-authoritative.

## Atomicity and interruption

A migration spanning multiple files should prepare and validate the complete target state before replacing canonical files where practical. An interrupted migration must be detectable from the filesystem and must never be presented as successfully complete merely because some files were rewritten.

For early versions, the safest implementation may be to write a separate migrated tree and replace the old tree only after validation.

## Downgrades

Downgrade support is not assumed. If a newer schema contains meaning that an older schema cannot represent, Allodium must refuse a lossy downgrade unless the user explicitly selects a documented lossy transformation.

## Projection compatibility

Adapters declare which canonical schema versions they understand. A provider adapter must not project a canonical object whose schema it does not understand.

Provider projections may evolve independently from canonical schemas. A GitHub API change does not itself require a canonical schema bump unless the canonical meaning changes.

## Git history is useful, not required for interpretation

Git may show when a migration happened and makes rollback convenient, but a checked-out filesystem tree must remain self-describing without consulting Git history. A user who receives the directory through another transport should still be able to identify every structured record's schema version.
