use crate::github::{GitHubOperation, GitHubPlan, PLAN_SCHEMA_V0};
use crate::release::{CanonicalRelease, load_releases};
use crate::load_remote;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const RELEASE_MAPPINGS_SCHEMA_V0: &str = "allodium.github.release-mappings/v0";
pub const OBSERVED_RELEASE_SCHEMA_V0: &str = "allodium.github.observed-release/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseMapping {
    pub id: u64,
    pub url: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseMappings {
    pub schema: String,
    #[serde(default)]
    pub releases: BTreeMap<String, ReleaseMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedRelease {
    pub schema: String,
    pub canonical_id: String,
    pub id: u64,
    pub url: String,
    pub name: String,
    pub tag_name: String,
    pub commit_sha: String,
    pub draft: bool,
    pub prerelease: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub observed_at: String,
}

pub fn plan_releases(root: impl AsRef<Path>, remote_name: &str) -> Result<GitHubPlan, String> {
    let root = root.as_ref();
    let remote = load_remote(root, remote_name)?;
    if remote.kind != "github" {
        return Err(format!(
            "remote {remote_name:?} has kind {:?}, not \"github\"",
            remote.kind
        ));
    }

    let mappings = load_release_mappings(root, remote_name)?;
    let releases = load_releases(root)?;
    let mut operations = Vec::new();

    for release in releases {
        plan_release(root, remote_name, &release, &mappings, &mut operations)?;
    }

    Ok(GitHubPlan {
        schema: PLAN_SCHEMA_V0.into(),
        remote: remote_name.into(),
        repository: remote.repository,
        operations,
    })
}

pub fn load_release_mappings(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<ReleaseMappings, String> {
    let path = release_mappings_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(ReleaseMappings {
            schema: RELEASE_MAPPINGS_SCHEMA_V0.into(),
            releases: BTreeMap::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: ReleaseMappings =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != RELEASE_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported release mapping schema {:?}; expected {:?}",
            path.display(),
            mappings.schema,
            RELEASE_MAPPINGS_SCHEMA_V0
        ));
    }
    Ok(mappings)
}

pub fn save_release_mappings(
    root: impl AsRef<Path>,
    remote_name: &str,
    mappings: &ReleaseMappings,
) -> Result<(), String> {
    if mappings.schema != RELEASE_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported release mapping schema {:?}",
            mappings.schema
        ));
    }
    let path = release_mappings_path(root.as_ref(), remote_name);
    fs::create_dir_all(path.parent().expect("mapping path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(mappings)
        .map_err(|error| format!("could not serialize GitHub release mappings: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn load_observed_release(
    root: impl AsRef<Path>,
    remote_name: &str,
    canonical_id: &str,
) -> Result<Option<(ObservedRelease, String)>, String> {
    let metadata_path = observed_release_path(root.as_ref(), remote_name, canonical_id);
    let notes_path = observed_release_notes_path(root.as_ref(), remote_name, canonical_id);
    if !metadata_path.exists() && !notes_path.exists() {
        return Ok(None);
    }
    if !metadata_path.exists() || !notes_path.exists() {
        return Err(format!(
            "observed GitHub Release snapshot for {canonical_id:?} is incomplete"
        ));
    }
    let text = fs::read_to_string(&metadata_path)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let observed: ObservedRelease = toml::from_str(&text)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    if observed.schema != OBSERVED_RELEASE_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported observed release schema {:?}; expected {:?}",
            metadata_path.display(),
            observed.schema,
            OBSERVED_RELEASE_SCHEMA_V0
        ));
    }
    let notes = fs::read_to_string(&notes_path)
        .map_err(|error| format!("{}: {error}", notes_path.display()))?;
    Ok(Some((observed, notes)))
}

pub fn write_observed_release_snapshot(
    root: impl AsRef<Path>,
    remote_name: &str,
    observed: &ObservedRelease,
    notes: &str,
) -> Result<(), String> {
    if observed.schema != OBSERVED_RELEASE_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed release schema {:?}",
            observed.schema
        ));
    }
    let metadata_path = observed_release_path(root.as_ref(), remote_name, &observed.canonical_id);
    let notes_path = observed_release_notes_path(root.as_ref(), remote_name, &observed.canonical_id);
    fs::create_dir_all(metadata_path.parent().expect("observed release path has parent"))
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let text = toml::to_string_pretty(observed)
        .map_err(|error| format!("could not serialize observed GitHub Release: {error}"))?;
    fs::write(&metadata_path, text)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    fs::write(&notes_path, notes).map_err(|error| format!("{}: {error}", notes_path.display()))
}

pub fn require_github_release_tag(release: &CanonicalRelease) -> Result<&str, String> {
    release
        .record
        .tag
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "GitHub Release projection v0 requires canonical release {:?} to declare an explicit tag",
                release.record.id
            )
        })
}

pub fn require_full_commit_sha(release: &CanonicalRelease) -> Result<&str, String> {
    let revision = release.record.revision.as_str();
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "GitHub Release projection v0 requires canonical release {:?} revision to be a full 40-hex commit SHA; got {:?}",
            release.record.id, release.record.revision
        ));
    }
    Ok(revision)
}

fn plan_release(
    root: &Path,
    remote_name: &str,
    release: &CanonicalRelease,
    mappings: &ReleaseMappings,
    operations: &mut Vec<GitHubOperation>,
) -> Result<(), String> {
    let canonical_id = &release.record.id;
    let tag = require_github_release_tag(release)?;
    let revision = require_full_commit_sha(release)?;

    let Some(mapping) = mappings.releases.get(canonical_id) else {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "create_release".into(),
            number: None,
            fields: vec![
                "title".into(),
                "notes".into(),
                "state".into(),
                "tag".into(),
                "revision".into(),
            ],
            reason: "canonical release has no GitHub Release mapping".into(),
        });
        return Ok(());
    };

    if mapping.tag != tag {
        return Err(format!(
            "canonical release {canonical_id:?} declares tag {tag:?}, but its GitHub mapping is bound to {:?}; GitHub Release tag identity is immutable in projection v0",
            mapping.tag
        ));
    }

    let Some((observed, observed_notes)) = load_observed_release(root, remote_name, canonical_id)? else {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "observe_release".into(),
            number: Some(mapping.id),
            fields: vec!["title".into(), "notes".into(), "state".into(), "tag".into(), "revision".into()],
            reason: "release mapping exists but the local observed GitHub Release snapshot is missing".into(),
        });
        return Ok(());
    };

    if observed.canonical_id != *canonical_id || observed.id != mapping.id {
        return Err(format!(
            "observed GitHub Release snapshot for {canonical_id:?} disagrees with its mapping"
        ));
    }
    if observed.tag_name != tag || observed.commit_sha != revision {
        return Err(format!(
            "GitHub Release identity drift for {canonical_id:?}: canonical tag/revision is {tag:?}@{revision}, observed provider identity is {:?}@{}; projection v0 refuses to retarget release history",
            observed.tag_name, observed.commit_sha
        ));
    }

    let mut fields = Vec::new();
    if observed.name != release.record.title {
        fields.push("title".into());
    }
    if observed_notes.trim_end() != release.notes.trim_end() {
        fields.push("notes".into());
    }
    let desired_draft = release.record.state == "draft";
    if observed.draft != desired_draft {
        fields.push("state".into());
    }
    if observed.prerelease {
        fields.push("prerelease".into());
    }

    if !fields.is_empty() {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "update_release".into(),
            number: Some(mapping.id),
            fields,
            reason: "canonical mutable release fields differ from the last observed GitHub Release state".into(),
        });
    }
    Ok(())
}

fn release_mappings_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("mappings/releases.toml")
}

fn observed_release_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/releases")
        .join(format!("{canonical_id}.toml"))
}

fn observed_release_notes_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/releases")
        .join(format!("{canonical_id}.notes.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn unmapped_release_plans_create() {
        let root = test_project("create", Some("v0.1.0"), SHA);
        let plan = plan_releases(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "create_release");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn github_projection_requires_explicit_tag() {
        let root = test_project("missing-tag", None, SHA);
        let error = plan_releases(&root, "github").unwrap_err();
        assert!(error.contains("explicit tag"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn github_projection_requires_full_commit_sha() {
        let root = test_project("short-sha", Some("v0.1.0"), "main");
        let error = plan_releases(&root, "github").unwrap_err();
        assert!(error.contains("full 40-hex commit SHA"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mapped_release_without_observation_plans_observe() {
        let root = test_project("observe", Some("v0.1.0"), SHA);
        save_mapping(&root, "v0.1.0");
        let plan = plan_releases(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "observe_release");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matching_release_is_idempotent() {
        let root = test_project("idempotent", Some("v0.1.0"), SHA);
        save_mapping(&root, "v0.1.0");
        save_observation(&root, "0.1.0", false, false, "Release notes\n");
        let plan = plan_releases(&root, "github").unwrap();
        assert!(plan.operations.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mutable_drift_plans_update() {
        let root = test_project("update", Some("v0.1.0"), SHA);
        save_mapping(&root, "v0.1.0");
        save_observation(&root, "Old title", true, true, "Old notes\n");
        let plan = plan_releases(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "update_release");
        assert!(plan.operations[0].fields.contains(&"title".into()));
        assert!(plan.operations[0].fields.contains(&"notes".into()));
        assert!(plan.operations[0].fields.contains(&"state".into()));
        assert!(plan.operations[0].fields.contains(&"prerelease".into()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn identity_drift_is_not_planned_as_mutable_update() {
        let root = test_project("identity-drift", Some("v0.1.0"), SHA);
        save_mapping(&root, "v0.1.0");
        let observed = ObservedRelease {
            schema: OBSERVED_RELEASE_SCHEMA_V0.into(),
            canonical_id: "release-0001".into(),
            id: 42,
            url: "https://example.invalid/release".into(),
            name: "0.1.0".into(),
            tag_name: "v0.1.0".into(),
            commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            draft: false,
            prerelease: false,
            published_at: Some("2026-09-16T00:00:00Z".into()),
            observed_at: "2026-09-16T00:00:00Z".into(),
        };
        write_observed_release_snapshot(&root, "github", &observed, "Release notes\n").unwrap();
        let error = plan_releases(&root, "github").unwrap_err();
        assert!(error.contains("refuses to retarget release history"));
        fs::remove_dir_all(root).unwrap();
    }

    fn save_mapping(root: &Path, tag: &str) {
        let mut releases = BTreeMap::new();
        releases.insert(
            "release-0001".into(),
            ReleaseMapping {
                id: 42,
                url: "https://example.invalid/release".into(),
                tag: tag.into(),
            },
        );
        save_release_mappings(
            root,
            "github",
            &ReleaseMappings {
                schema: RELEASE_MAPPINGS_SCHEMA_V0.into(),
                releases,
            },
        )
        .unwrap();
    }

    fn save_observation(root: &Path, name: &str, draft: bool, prerelease: bool, notes: &str) {
        let observed = ObservedRelease {
            schema: OBSERVED_RELEASE_SCHEMA_V0.into(),
            canonical_id: "release-0001".into(),
            id: 42,
            url: "https://example.invalid/release".into(),
            name: name.into(),
            tag_name: "v0.1.0".into(),
            commit_sha: SHA.into(),
            draft,
            prerelease,
            published_at: None,
            observed_at: "2026-09-16T00:00:00Z".into(),
        };
        write_observed_release_snapshot(root, "github", &observed, notes).unwrap();
    }

    fn test_project(name: &str, tag: Option<&str>, revision: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-github-release-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".project/issues/issue-0001")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::create_dir_all(root.join(".project/releases/release-0001")).unwrap();
        fs::write(
            root.join(".project/manifest.toml"),
            "schema = \"allodium.project/v0\"\nid = \"test\"\nname = \"Test\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/issues/issue-0001/issue.toml"),
            "schema = \"allodium.issue/v0\"\nid = \"issue-0001\"\ntitle = \"Test\"\nstate = \"open\"\n",
        )
        .unwrap();
        fs::write(root.join(".project/issues/issue-0001/body.md"), "Body\n").unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        let tag_line = tag.map(|tag| format!("tag = \"{tag}\"\n")).unwrap_or_default();
        fs::write(
            root.join(".project/releases/release-0001/release.toml"),
            format!(
                "schema = \"allodium.release/v0\"\nid = \"release-0001\"\ntitle = \"0.1.0\"\nversion = \"0.1.0\"\nstate = \"published\"\nrevision = \"{revision}\"\n{tag_line}"
            ),
        )
        .unwrap();
        fs::write(
            root.join(".project/releases/release-0001/notes.md"),
            "Release notes\n",
        )
        .unwrap();
        root
    }
}
