use super::{GitHubAdapter, incoming_directory, now, timestamp_slug, write_toml};
use allodium_core::github_release::{
    OBSERVED_RELEASE_SCHEMA_V0, ObservedRelease, ReleaseMapping, load_observed_release,
    load_release_mappings, require_full_commit_sha, require_github_release_tag,
    save_release_mappings, write_observed_release_snapshot,
};
use allodium_core::release::CanonicalRelease;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;

const RELEASE_CHANGE_SCHEMA_V0: &str = "allodium.github.release-managed-change-observation/v0";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ReleaseObserveReport {
    pub observed: usize,
    pub managed_changes_archived: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiRelease {
    id: u64,
    html_url: String,
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiCommit {
    sha: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiRepositoryTag {
    name: String,
    commit: ApiCommit,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiGitObject {
    sha: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiGitRef {
    #[serde(rename = "ref")]
    git_ref: String,
    object: ApiGitObject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReleaseManagedChangeEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    canonical_id: String,
    release_id: u64,
    url: String,
    fields: Vec<String>,
}

pub(super) fn observe_releases(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<ReleaseObserveReport, String> {
    let mappings = load_release_mappings(root, remote_name)?;
    let mut report = ReleaseObserveReport::default();

    for (canonical_id, mapping) in mappings.releases {
        let observed_at = now();
        let live = fetch_release(adapter, mapping.id)?;
        let (snapshot, notes) =
            snapshot_from_api(adapter, &canonical_id, &live, &mapping.tag, &observed_at)?;
        report.managed_changes_archived +=
            archive_managed_change_if_needed(root, remote_name, &snapshot, &notes)?;
        write_observed_release_snapshot(root, remote_name, &snapshot, &notes)?;
        report.observed += 1;
    }

    Ok(report)
}

pub(super) fn apply_create_release(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    release: &CanonicalRelease,
) -> Result<(), String> {
    adapter.require_write_token()?;
    let tag = require_github_release_tag(release)?;
    let revision = require_full_commit_sha(release)?;
    let mut mappings = load_release_mappings(root, remote_name)?;
    if mappings.releases.contains_key(&release.record.id) {
        return Err(format!(
            "refusing create: canonical release {:?} already has a GitHub Release mapping",
            release.record.id
        ));
    }

    verify_canonical_revision(adapter, release)?;
    ensure_tag_at_revision(adapter, tag, revision)?;

    let created: ApiRelease = adapter.post(
        &format!("/repos/{}/releases", adapter.repository),
        &json!({
            "tag_name": tag,
            "target_commitish": revision,
            "name": release.record.title,
            "body": release.notes,
            "draft": release.record.state == "draft",
            "prerelease": false,
            "generate_release_notes": false,
            "make_latest": "false",
        }),
    )?;
    if created.tag_name != tag {
        return Err(format!(
            "GitHub created release {} with tag {:?}, expected canonical tag {tag:?}; refusing to record a mapping",
            created.id, created.tag_name
        ));
    }

    let observed_at = now();
    let (snapshot, notes) =
        snapshot_from_api(adapter, &release.record.id, &created, tag, &observed_at)?;
    assert_provider_identity_matches_canonical(release, &snapshot)?;

    mappings.releases.insert(
        release.record.id.clone(),
        ReleaseMapping {
            id: created.id,
            url: created.html_url.clone(),
            tag: tag.into(),
        },
    );
    save_release_mappings(root, remote_name, &mappings)?;
    write_observed_release_snapshot(root, remote_name, &snapshot, &notes)
}

pub(super) fn apply_observe_release(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    release_id: u64,
) -> Result<(), String> {
    let mappings = load_release_mappings(root, remote_name)?;
    let mapping = mappings.releases.get(canonical_id).ok_or_else(|| {
        format!(
            "cannot observe GitHub Release {release_id}: no mapping exists for {canonical_id:?}"
        )
    })?;
    if mapping.id != release_id {
        return Err(format!(
            "GitHub Release operation id {release_id} disagrees with mapping {} for {canonical_id:?}",
            mapping.id
        ));
    }

    let live = fetch_release(adapter, release_id)?;
    let observed_at = now();
    let (snapshot, notes) =
        snapshot_from_api(adapter, canonical_id, &live, &mapping.tag, &observed_at)?;
    archive_managed_change_if_needed(root, remote_name, &snapshot, &notes)?;
    write_observed_release_snapshot(root, remote_name, &snapshot, &notes)
}

pub(super) fn apply_update_release(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    release: &CanonicalRelease,
    release_id: u64,
    fields: &[String],
) -> Result<(), String> {
    adapter.require_write_token()?;
    let canonical_tag = require_github_release_tag(release)?;
    let canonical_revision = require_full_commit_sha(release)?;
    let (previous, previous_notes) = load_observed_release(root, remote_name, &release.record.id)?
        .ok_or_else(|| {
            format!(
                "refusing update of {:?}: no observed GitHub Release snapshot; observe and re-plan before applying",
                release.record.id
            )
        })?;
    if previous.id != release_id {
        return Err(format!(
            "refusing update of {:?}: plan release id {release_id} disagrees with observed id {}",
            release.record.id, previous.id
        ));
    }

    let live = fetch_release(adapter, release_id)?;
    let live_observed_at = now();
    let (live_snapshot, live_notes) = snapshot_from_api(
        adapter,
        &release.record.id,
        &live,
        canonical_tag,
        &live_observed_at,
    )?;
    if !same_remote_state(&previous, &previous_notes, &live_snapshot, &live_notes) {
        return Err(format!(
            "refusing stale update of {:?}: GitHub Release changed after the recorded observation; observe and re-plan before applying",
            release.record.id
        ));
    }
    if !live_snapshot
        .commit_sha
        .eq_ignore_ascii_case(canonical_revision)
    {
        return Err(format!(
            "refusing update of {:?}: mapped canonical tag {canonical_tag:?} resolves to {}, expected {canonical_revision}",
            release.record.id, live_snapshot.commit_sha
        ));
    }

    let payload = update_payload(release, fields)?;
    let updated: ApiRelease = adapter.patch(
        &format!("/repos/{}/releases/{release_id}", adapter.repository),
        &payload,
    )?;
    let observed_at = now();
    let (snapshot, notes) = snapshot_from_api(
        adapter,
        &release.record.id,
        &updated,
        canonical_tag,
        &observed_at,
    )?;
    assert_provider_identity_matches_canonical(release, &snapshot)?;
    write_observed_release_snapshot(root, remote_name, &snapshot, &notes)
}

fn verify_canonical_revision(
    adapter: &GitHubAdapter,
    release: &CanonicalRelease,
) -> Result<(), String> {
    let revision = require_full_commit_sha(release)?;
    let commit: ApiCommit =
        adapter.get(&format!("/repos/{}/commits/{revision}", adapter.repository))?;
    if !commit.sha.eq_ignore_ascii_case(revision) {
        return Err(format!(
            "canonical release {:?} revision {:?} resolved on GitHub to {}; refusing release projection",
            release.record.id, revision, commit.sha
        ));
    }
    Ok(())
}

fn ensure_tag_at_revision(
    adapter: &GitHubAdapter,
    tag: &str,
    revision: &str,
) -> Result<(), String> {
    if let Some(existing) = find_repository_tag(adapter, tag)? {
        if !existing.commit.sha.eq_ignore_ascii_case(revision) {
            return Err(format!(
                "refusing GitHub Release projection: existing tag {tag:?} resolves to {}, canonical release requires {revision}",
                existing.commit.sha
            ));
        }
        return Ok(());
    }

    let created: ApiGitRef = adapter.post(
        &format!("/repos/{}/git/refs", adapter.repository),
        &json!({
            "ref": format!("refs/tags/{tag}"),
            "sha": revision,
        }),
    )?;
    if created.git_ref != format!("refs/tags/{tag}")
        || !created.object.sha.eq_ignore_ascii_case(revision)
    {
        return Err(format!(
            "GitHub created unexpected tag reference {:?} -> {}; expected refs/tags/{tag} -> {revision}",
            created.git_ref, created.object.sha
        ));
    }
    Ok(())
}

fn find_repository_tag(
    adapter: &GitHubAdapter,
    tag: &str,
) -> Result<Option<ApiRepositoryTag>, String> {
    let mut page = 1;
    loop {
        let batch: Vec<ApiRepositoryTag> = adapter.get(&format!(
            "/repos/{}/tags?per_page=100&page={page}",
            adapter.repository
        ))?;
        let count = batch.len();
        if let Some(found) = batch.into_iter().find(|candidate| candidate.name == tag) {
            return Ok(Some(found));
        }
        if count < 100 {
            return Ok(None);
        }
        page += 1;
    }
}

fn fetch_release(adapter: &GitHubAdapter, release_id: u64) -> Result<ApiRelease, String> {
    adapter.get(&format!(
        "/repos/{}/releases/{release_id}",
        adapter.repository
    ))
}

fn snapshot_from_api(
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
    let notes = release.body.clone().unwrap_or_default();
    Ok((
        ObservedRelease {
            schema: OBSERVED_RELEASE_SCHEMA_V0.into(),
            canonical_id: canonical_id.into(),
            id: release.id,
            url: release.html_url.clone(),
            name: release.name.clone().unwrap_or_default(),
            tag_name: release.tag_name.clone(),
            commit_sha: tag.commit.sha,
            draft: release.draft,
            prerelease: release.prerelease,
            published_at: release.published_at.clone(),
            observed_at: observed_at.into(),
        },
        notes,
    ))
}

fn assert_provider_identity_matches_canonical(
    release: &CanonicalRelease,
    observed: &ObservedRelease,
) -> Result<(), String> {
    let tag = require_github_release_tag(release)?;
    let revision = require_full_commit_sha(release)?;
    if observed.tag_name != tag || !observed.commit_sha.eq_ignore_ascii_case(revision) {
        return Err(format!(
            "GitHub Release identity drift for {:?}: canonical identity is {tag:?}@{revision}, provider identity is {:?}@{}; projection v0 refuses to retarget release history",
            release.record.id, observed.tag_name, observed.commit_sha
        ));
    }
    Ok(())
}

fn update_payload(
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
        match field.as_str() {
            "title" => {
                object.insert("name".into(), json!(release.record.title));
            }
            "notes" => {
                object.insert("body".into(), json!(release.notes));
            }
            "state" => {
                object.insert("draft".into(), json!(release.record.state == "draft"));
                object.insert("make_latest".into(), json!("false"));
            }
            "prerelease" => {
                object.insert("prerelease".into(), json!(false));
            }
            "tag" | "revision" => {
                // Reasserted unconditionally above; never retargeted from provider state.
            }
            other => {
                return Err(format!(
                    "GitHub Release projection v0 cannot update field {other:?}; tag/revision identity is immutable"
                ));
            }
        }
    }
    Ok(serde_json::Value::Object(object))
}

fn archive_managed_change_if_needed(
    root: &Path,
    remote_name: &str,
    current: &ObservedRelease,
    current_notes: &str,
) -> Result<usize, String> {
    let Some((previous, previous_notes)) =
        load_observed_release(root, remote_name, &current.canonical_id)?
    else {
        return Ok(0);
    };
    let fields = changed_fields(&previous, &previous_notes, current, current_notes);
    if fields.is_empty() {
        return Ok(0);
    }

    let event_id = format!(
        "{remote_name}-release-{}-managed-change-{}",
        current.id,
        timestamp_slug(&current.observed_at)
    );
    let directory = incoming_directory(root, remote_name, &current.observed_at, &event_id)?;
    let event = ReleaseManagedChangeEvent {
        schema: RELEASE_CHANGE_SCHEMA_V0.into(),
        id: event_id,
        remote: remote_name.into(),
        kind: "release.managed_fields.changed".into(),
        observed_at: current.observed_at.clone(),
        evidence: "GitHub REST Release observation plus resolved Git tag commit; actor is not asserted because this surface does not prove the editor identity.".into(),
        canonical_id: current.canonical_id.clone(),
        release_id: current.id,
        url: current.url.clone(),
        fields,
    };

    if directory.exists() {
        return Err(format!(
            "GitHub Release managed-change evidence collision at {}",
            directory.display()
        ));
    }
    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    write_toml(directory.join("event.toml"), &event)?;
    write_toml(directory.join("before.toml"), &previous)?;
    fs::write(directory.join("before.notes.md"), previous_notes)
        .map_err(|error| format!("could not preserve previous GitHub Release notes: {error}"))?;
    write_toml(directory.join("after.toml"), current)?;
    fs::write(directory.join("after.notes.md"), current_notes)
        .map_err(|error| format!("could not preserve new GitHub Release notes: {error}"))?;
    Ok(1)
}

fn changed_fields(
    previous: &ObservedRelease,
    previous_notes: &str,
    current: &ObservedRelease,
    current_notes: &str,
) -> Vec<String> {
    let mut fields = Vec::new();
    if previous.name != current.name {
        fields.push("title".into());
    }
    if previous_notes != current_notes {
        fields.push("notes".into());
    }
    if previous.draft != current.draft {
        fields.push("state".into());
    }
    if previous.prerelease != current.prerelease {
        fields.push("prerelease".into());
    }
    if previous.tag_name != current.tag_name {
        fields.push("tag".into());
    }
    if previous.commit_sha != current.commit_sha {
        fields.push("revision".into());
    }
    if previous.published_at != current.published_at {
        fields.push("published_at".into());
    }
    fields
}

fn same_remote_state(
    previous: &ObservedRelease,
    previous_notes: &str,
    current: &ObservedRelease,
    current_notes: &str,
) -> bool {
    previous.canonical_id == current.canonical_id
        && previous.id == current.id
        && previous.url == current.url
        && previous.name == current.name
        && previous.tag_name == current.tag_name
        && previous.commit_sha == current.commit_sha
        && previous.draft == current.draft
        && previous.prerelease == current.prerelease
        && previous.published_at == current.published_at
        && previous_notes == current_notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed() -> ObservedRelease {
        ObservedRelease {
            schema: OBSERVED_RELEASE_SCHEMA_V0.into(),
            canonical_id: "release-0001".into(),
            id: 42,
            url: "https://example.invalid/release".into(),
            name: "0.1.0".into(),
            tag_name: "v0.1.0".into(),
            commit_sha: "0123456789abcdef0123456789abcdef01234567".into(),
            draft: true,
            prerelease: false,
            published_at: None,
            observed_at: "2026-09-16T00:00:00Z".into(),
        }
    }

    #[test]
    fn observation_timestamp_is_not_remote_state() {
        let first = observed();
        let mut second = first.clone();
        second.observed_at = "2026-09-16T01:00:00Z".into();
        assert!(same_remote_state(&first, "notes", &second, "notes"));
    }

    #[test]
    fn release_change_detection_includes_identity_drift() {
        let first = observed();
        let mut second = first.clone();
        second.commit_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
        assert_eq!(
            changed_fields(&first, "notes", &second, "notes"),
            vec!["revision"]
        );
    }

    #[test]
    fn published_payload_does_not_claim_latest_release() {
        let release = CanonicalRelease {
            record: allodium_core::release::ReleaseRecord {
                schema: allodium_core::release::RELEASE_SCHEMA_V0.into(),
                id: "release-0001".into(),
                title: "0.1.0".into(),
                version: "0.1.0".into(),
                state: "published".into(),
                revision: "0123456789abcdef0123456789abcdef01234567".into(),
                tag: Some("v0.1.0".into()),
            },
            notes: "notes".into(),
            directory: std::path::PathBuf::new(),
        };
        let payload = update_payload(&release, &["state".into()]).unwrap();
        assert_eq!(payload["draft"], false);
        assert_eq!(payload["make_latest"], "false");
    }

    #[test]
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
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::{SystemTime, UNIX_EPOCH};

        const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&requests);
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 8192];
                let count = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let line = request.lines().next().unwrap_or_default().to_string();
                seen.lock().unwrap().push(line.clone());
                let body = if line.contains("/releases/42 ") {
                    format!(
                        r#"{{"id":42,"html_url":"https://example.invalid/release","tag_name":"v0.1.0","name":"Externally changed","body":"notes","draft":true,"prerelease":false,"published_at":null}}"#
                    )
                } else {
                    format!(r#"[{{"name":"v0.1.0","commit":{{"sha":"{SHA}"}}}}]"#)
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        });

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-release-stale-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let previous = ObservedRelease {
            schema: OBSERVED_RELEASE_SCHEMA_V0.into(),
            canonical_id: "release-0001".into(),
            id: 42,
            url: "https://example.invalid/release".into(),
            name: "Previously observed".into(),
            tag_name: "v0.1.0".into(),
            commit_sha: SHA.into(),
            draft: true,
            prerelease: false,
            published_at: None,
            observed_at: "2026-09-16T00:00:00Z".into(),
        };
        write_observed_release_snapshot(&root, "github", &previous, "notes").unwrap();
        let canonical = CanonicalRelease {
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
        let adapter = GitHubAdapter {
            client: reqwest::blocking::Client::builder().build().unwrap(),
            repository: "owner/repo".into(),
            token: Some("test-token".into()),
            api_base: format!("http://{address}"),
        };

        let error =
            apply_update_release(&adapter, &root, "github", &canonical, 42, &["title".into()])
                .unwrap_err();
        assert!(error.contains("refusing stale update"), "{error}");
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| request.starts_with("GET ")));
        std::fs::remove_dir_all(root).unwrap();
    }
}
