from pathlib import Path

core = Path('crates/allodium-core/src/github_release.rs')
text = core.read_text()
old = '''    if observed.tag_name != tag || observed.commit_sha != revision {
        return Err(format!(
            "GitHub Release identity drift for {canonical_id:?}: canonical tag/revision is {tag:?}@{revision}, observed provider identity is {:?}@{}; projection v0 refuses to retarget release history",
            observed.tag_name, observed.commit_sha
        ));
    }

    let mut fields = Vec::new();
'''
new = '''    if !observed.commit_sha.eq_ignore_ascii_case(revision) {
        return Err(format!(
            "GitHub Release immutable identity drift for {canonical_id:?}: mapped tag {tag:?} now resolves to {}, canonical revision is {revision}; projection v0 refuses to retarget release history",
            observed.commit_sha
        ));
    }

    let mut fields = Vec::new();
    if observed.tag_name != tag {
        fields.push("tag".into());
    }
'''
if old not in text:
    raise SystemExit('missing core release identity planning anchor')
text = text.replace(old, new, 1)

test_anchor = '''    #[test]
    fn identity_drift_is_not_planned_as_mutable_update() {
'''
test_insert = '''    #[test]
    fn provider_attachment_tag_drift_plans_repair_when_mapped_tag_anchor_is_intact() {
        let root = test_project("attachment-tag-drift", Some("v0.1.0"), SHA);
        save_mapping(&root, "v0.1.0");
        let observed = ObservedRelease {
            schema: OBSERVED_RELEASE_SCHEMA_V0.into(),
            canonical_id: "release-0001".into(),
            id: 42,
            url: "https://example.invalid/release".into(),
            name: "0.1.0".into(),
            tag_name: "untagged-provider-slug".into(),
            commit_sha: SHA.into(),
            draft: false,
            prerelease: false,
            published_at: Some("2026-09-16T00:00:00Z".into()),
            observed_at: "2026-09-16T00:00:00Z".into(),
        };
        write_observed_release_snapshot(&root, "github", &observed, "Release notes\n").unwrap();

        let plan = plan_releases(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "update_release");
        assert!(plan.operations[0].fields.contains(&"tag".into()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn identity_drift_is_not_planned_as_mutable_update() {
'''
if test_anchor not in text:
    raise SystemExit('missing core release identity test anchor')
core.write_text(text.replace(test_anchor, test_insert, 1))

runtime = Path('crates/allodium-github/src/release.rs')
text = runtime.read_text()
text = text.replace(
    'use super::{GitHubAdapter, github_api_error, incoming_directory, now, timestamp_slug, write_toml};',
    'use super::{GitHubAdapter, incoming_directory, now, timestamp_slug, write_toml};',
    1,
)
text = text.replace(
    '    OBSERVED_RELEASE_SCHEMA_V0, ObservedRelease, ReleaseMapping, ReleaseMappings,\n',
    '    OBSERVED_RELEASE_SCHEMA_V0, ObservedRelease, ReleaseMapping,\n',
    1,
)

old = '''        let live = fetch_release(adapter, mapping.id)?;
        if live.tag_name != mapping.tag {
            return Err(format!(
                "GitHub Release {} mapped to {canonical_id:?} now names tag {:?}, but the durable mapping is bound to {:?}",
                mapping.id, live.tag_name, mapping.tag
            ));
        }
        let (snapshot, notes) = snapshot_from_api(adapter, &canonical_id, &live, &observed_at)?;
'''
new = '''        let live = fetch_release(adapter, mapping.id)?;
        let (snapshot, notes) = snapshot_from_api(
            adapter,
            &canonical_id,
            &live,
            &mapping.tag,
            &observed_at,
        )?;
'''
if old not in text:
    raise SystemExit('missing runtime observe release anchor')
text = text.replace(old, new, 1)

old = '''    let (snapshot, notes) = snapshot_from_api(adapter, &release.record.id, &created, &observed_at)?;
'''
new = '''    let (snapshot, notes) = snapshot_from_api(
        adapter,
        &release.record.id,
        &created,
        tag,
        &observed_at,
    )?;
'''
if old not in text:
    raise SystemExit('missing runtime create snapshot anchor')
text = text.replace(old, new, 1)

old = '''    let live = fetch_release(adapter, release_id)?;
    let observed_at = now();
    let (snapshot, notes) = snapshot_from_api(adapter, canonical_id, &live, &observed_at)?;
'''
new = '''    let live = fetch_release(adapter, release_id)?;
    let observed_at = now();
    let (snapshot, notes) = snapshot_from_api(
        adapter,
        canonical_id,
        &live,
        &mapping.tag,
        &observed_at,
    )?;
'''
if old not in text:
    raise SystemExit('missing runtime observe-operation snapshot anchor')
text = text.replace(old, new, 1)

old = '''    adapter.require_write_token()?;
    let (previous, previous_notes) = load_observed_release(root, remote_name, &release.record.id)?
'''
new = '''    adapter.require_write_token()?;
    let canonical_tag = require_github_release_tag(release)?;
    let canonical_revision = require_full_commit_sha(release)?;
    let (previous, previous_notes) = load_observed_release(root, remote_name, &release.record.id)?
'''
if old not in text:
    raise SystemExit('missing runtime update prelude anchor')
text = text.replace(old, new, 1)

old = '''    let (live_snapshot, live_notes) =
        snapshot_from_api(adapter, &release.record.id, &live, &live_observed_at)?;
'''
new = '''    let (live_snapshot, live_notes) = snapshot_from_api(
        adapter,
        &release.record.id,
        &live,
        canonical_tag,
        &live_observed_at,
    )?;
'''
if old not in text:
    raise SystemExit('missing runtime live snapshot anchor')
text = text.replace(old, new, 1)

old = '''    assert_provider_identity_matches_canonical(release, &live_snapshot)?;

    let payload = update_payload(release, fields)?;
'''
new = '''    if !live_snapshot
        .commit_sha
        .eq_ignore_ascii_case(canonical_revision)
    {
        return Err(format!(
            "refusing update of {:?}: mapped canonical tag {canonical_tag:?} resolves to {}, expected {canonical_revision}",
            release.record.id, live_snapshot.commit_sha
        ));
    }

    let payload = update_payload(release, fields)?;
'''
if old not in text:
    raise SystemExit('missing runtime pre-update identity assertion anchor')
text = text.replace(old, new, 1)

old = '''    let (snapshot, notes) = snapshot_from_api(adapter, &release.record.id, &updated, &observed_at)?;
'''
new = '''    let (snapshot, notes) = snapshot_from_api(
        adapter,
        &release.record.id,
        &updated,
        canonical_tag,
        &observed_at,
    )?;
'''
if old not in text:
    raise SystemExit('missing runtime updated snapshot anchor')
text = text.replace(old, new, 1)

old = '''fn snapshot_from_api(
    adapter: &GitHubAdapter,
    canonical_id: &str,
    release: &ApiRelease,
    observed_at: &str,
) -> Result<(ObservedRelease, String), String> {
    let tag = find_repository_tag(adapter, &release.tag_name)?.ok_or_else(|| {
        format!(
            "GitHub Release {} names tag {:?}, but that Git tag is not materialized; Allodium cannot establish immutable release identity",
            release.id, release.tag_name
        )
    })?;
'''
new = '''fn snapshot_from_api(
    adapter: &GitHubAdapter,
    canonical_id: &str,
    release: &ApiRelease,
    identity_tag: &str,
    observed_at: &str,
) -> Result<(ObservedRelease, String), String> {
    let tag = find_repository_tag(adapter, identity_tag)?.ok_or_else(|| {
        format!(
            "GitHub Release {} is mapped through tag {identity_tag:?}, but that Git tag is not materialized; Allodium cannot establish immutable release identity",
            release.id
        )
    })?;
'''
if old not in text:
    raise SystemExit('missing runtime snapshot function anchor')
text = text.replace(old, new, 1)

old = '''fn update_payload(
    release: &CanonicalRelease,
    fields: &[String],
) -> Result<serde_json::Value, String> {
    let mut object = serde_json::Map::new();
    for field in fields {
'''
new = '''fn update_payload(
    release: &CanonicalRelease,
    fields: &[String],
) -> Result<serde_json::Value, String> {
    let tag = require_github_release_tag(release)?;
    let revision = require_full_commit_sha(release)?;
    let mut object = serde_json::Map::new();
    // GitHub draft releases can silently fall back to an internal `untagged-*`
    // attachment when PATCH omits identity fields. Every mutable update therefore
    // reasserts the canonical attachment while the actual Git ref remains the
    // immutable identity anchor.
    object.insert("tag_name".into(), json!(tag));
    object.insert("target_commitish".into(), json!(revision));
    for field in fields {
'''
if old not in text:
    raise SystemExit('missing runtime update payload prelude anchor')
text = text.replace(old, new, 1)

old = '''            "prerelease" => {
                object.insert("prerelease".into(), json!(false));
            }
            other => {
'''
new = '''            "prerelease" => {
                object.insert("prerelease".into(), json!(false));
            }
            "tag" | "revision" => {
                // Reasserted unconditionally above; never retargeted from provider state.
            }
            other => {
'''
if old not in text:
    raise SystemExit('missing runtime update payload field anchor')
text = text.replace(old, new, 1)

old = '''    if previous.draft != current.draft {
        fields.push("state".into());
    }
'''
new = '''    if previous.draft != current.draft {
        fields.push("state".into());
    }
    if previous.tag_name != current.tag_name {
        fields.push("tag".into());
    }
    if !previous.commit_sha.eq_ignore_ascii_case(&current.commit_sha) {
        fields.push("revision".into());
    }
'''
if old not in text:
    raise SystemExit('missing runtime changed fields anchor')
text = text.replace(old, new, 1)

test_anchor = '''    #[test]
    fn stale_release_update_sends_no_patch() {
'''
test_insert = '''    #[test]
    fn mutable_update_reasserts_canonical_release_attachment_identity() {
        const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
        let release = CanonicalRelease {
            record: allodium_core::release::ReleaseRecord {
                schema: allodium_core::release::RELEASE_SCHEMA_V0.into(),
                id: "release-0001".into(),
                title: "Canonical title".into(),
                version: "0.1.0".into(),
                state: "draft".into(),
                revision: SHA.into(),
                tag: Some("v0.1.0".into()),
            },
            notes: "notes".into(),
            directory: std::path::PathBuf::new(),
        };
        let payload = update_payload(&release, &["title".into()]).unwrap();
        assert_eq!(payload["name"], "Canonical title");
        assert_eq!(payload["tag_name"], "v0.1.0");
        assert_eq!(payload["target_commitish"], SHA);
    }

    #[test]
    fn stale_release_update_sends_no_patch() {
'''
if test_anchor not in text:
    raise SystemExit('missing runtime identity regression test anchor')
runtime.write_text(text.replace(test_anchor, test_insert, 1))
