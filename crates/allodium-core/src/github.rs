use crate::{CanonicalIssue, load_issues, load_remote};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const ISSUE_MAPPINGS_SCHEMA_V0: &str = "allodium.github.issue-mappings/v0";
pub const OBSERVED_ISSUE_SCHEMA_V0: &str = "allodium.github.observed-issue/v0";
pub const PLAN_SCHEMA_V0: &str = "allodium.github.plan/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueMapping {
    pub number: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueMappings {
    pub schema: String,
    #[serde(default)]
    pub issues: BTreeMap<String, IssueMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedIssue {
    pub schema: String,
    pub canonical_id: String,
    pub number: u64,
    pub state: String,
    pub title: String,
    pub url: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubPlan {
    pub schema: String,
    pub remote: String,
    pub repository: String,
    #[serde(default)]
    pub operations: Vec<GitHubOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubOperation {
    pub canonical_id: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    pub reason: String,
}

pub fn plan_issues(root: impl AsRef<Path>, remote_name: &str) -> Result<GitHubPlan, String> {
    let root = root.as_ref();
    let remote = load_remote(root, remote_name)?;
    if remote.kind != "github" {
        return Err(format!(
            "remote {remote_name:?} has kind {:?}, not \"github\"",
            remote.kind
        ));
    }

    let mappings = load_issue_mappings(root, remote_name)?;
    let issues = load_issues(root)?;
    let mut operations = Vec::new();

    for issue in issues {
        plan_issue(root, remote_name, &issue, &mappings, &mut operations)?;
    }

    Ok(GitHubPlan {
        schema: PLAN_SCHEMA_V0.into(),
        remote: remote_name.into(),
        repository: remote.repository,
        operations,
    })
}

pub fn plan_to_toml(plan: &GitHubPlan) -> Result<String, String> {
    toml::to_string_pretty(plan).map_err(|error| format!("could not serialize plan: {error}"))
}

pub fn render_issue_body(issue: &CanonicalIssue) -> String {
    format!(
        "{}\n\n---\n\n**Allodium canonical ID:** `{}`\n\nThis GitHub issue is a projection of `.project/issues/{}/`, not the canonical record.",
        issue.body.trim(),
        issue.record.id,
        issue.record.id
    )
}

fn plan_issue(
    root: &Path,
    remote_name: &str,
    issue: &CanonicalIssue,
    mappings: &IssueMappings,
    operations: &mut Vec<GitHubOperation>,
) -> Result<(), String> {
    let canonical_id = &issue.record.id;
    let Some(mapping) = mappings.issues.get(canonical_id) else {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "create_issue".into(),
            number: None,
            fields: vec!["title".into(), "body".into(), "state".into()],
            reason: "canonical issue has no GitHub mapping".into(),
        });
        return Ok(());
    };

    let observed_path = observed_issue_path(root, remote_name, canonical_id);
    let observed_body_path = observed_issue_body_path(root, remote_name, canonical_id);
    if !observed_path.exists() || !observed_body_path.exists() {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "observe_issue".into(),
            number: Some(mapping.number),
            fields: vec!["title".into(), "body".into(), "state".into()],
            reason: "mapping exists but the local observed snapshot is incomplete".into(),
        });
        return Ok(());
    }

    let observed = load_observed_issue(&observed_path)?;
    if observed.canonical_id != *canonical_id {
        return Err(format!(
            "{}: canonical_id {:?} does not match {:?}",
            observed_path.display(),
            observed.canonical_id,
            canonical_id
        ));
    }
    if observed.number != mapping.number {
        return Err(format!(
            "{}: GitHub issue number {} disagrees with mapping {}",
            observed_path.display(),
            observed.number,
            mapping.number
        ));
    }

    let observed_body = fs::read_to_string(&observed_body_path)
        .map_err(|error| format!("{}: {error}", observed_body_path.display()))?;
    let desired_body = render_issue_body(issue);
    let mut fields = Vec::new();

    if observed.title != issue.record.title {
        fields.push("title".into());
    }
    if observed.state != issue.record.state {
        fields.push("state".into());
    }
    if observed_body.trim_end() != desired_body.trim_end() {
        fields.push("body".into());
    }

    if !fields.is_empty() {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "update_issue".into(),
            number: Some(mapping.number),
            fields,
            reason: "canonical managed fields differ from the last observed GitHub state".into(),
        });
    }

    Ok(())
}

fn load_issue_mappings(root: &Path, remote_name: &str) -> Result<IssueMappings, String> {
    let path = root
        .join(".project/remotes")
        .join(remote_name)
        .join("mappings/issues.toml");
    if !path.exists() {
        return Ok(IssueMappings {
            schema: ISSUE_MAPPINGS_SCHEMA_V0.into(),
            issues: BTreeMap::new(),
        });
    }

    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: IssueMappings =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != ISSUE_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported schema {:?}; expected {:?}",
            path.display(),
            mappings.schema,
            ISSUE_MAPPINGS_SCHEMA_V0
        ));
    }
    Ok(mappings)
}

fn load_observed_issue(path: &Path) -> Result<ObservedIssue, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedIssue =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_ISSUE_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported schema {:?}; expected {:?}",
            path.display(),
            observed.schema,
            OBSERVED_ISSUE_SCHEMA_V0
        ));
    }
    Ok(observed)
}

fn observed_issue_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/issues")
        .join(format!("{canonical_id}.toml"))
}

fn observed_issue_body_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/issues")
        .join(format!("{canonical_id}.body.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn unmapped_issue_plans_create() {
        let root = test_project("create");

        let plan = plan_issues(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "create_issue");
        assert_eq!(plan.operations[0].canonical_id, "issue-0001");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mapped_but_unobserved_issue_plans_observation() {
        let root = test_project("observe");
        write_mapping(&root);

        let plan = plan_issues(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "observe_issue");
        assert_eq!(plan.operations[0].number, Some(17));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matching_observation_is_idempotent() {
        let root = test_project("idempotent");
        write_mapping(&root);
        write_matching_observation(&root);

        let plan = plan_issues(&root, "github").unwrap();
        assert!(plan.operations.is_empty(), "{:?}", plan.operations);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_managed_field_plans_update() {
        let root = test_project("update");
        write_mapping(&root);
        write_matching_observation(&root);
        fs::write(
            root.join(".project/issues/issue-0001/issue.toml"),
            "schema = \"allodium.issue/v0\"\nid = \"issue-0001\"\ntitle = \"Changed\"\nstate = \"open\"\n",
        )
        .unwrap();

        let plan = plan_issues(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "update_issue");
        assert_eq!(plan.operations[0].fields, vec!["title"]);

        fs::remove_dir_all(root).unwrap();
    }

    fn test_project(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-github-{name}-{}-{nonce}",
            std::process::id()
        ));

        fs::create_dir_all(root.join(".project/issues/issue-0001")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github/mappings")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github/observed/issues")).unwrap();
        fs::write(
            root.join(".project/manifest.toml"),
            "schema = \"allodium.project/v0\"\nid = \"test\"\nname = \"Test\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/issues/issue-0001/issue.toml"),
            "schema = \"allodium.issue/v0\"\nid = \"issue-0001\"\ntitle = \"Test issue\"\nstate = \"open\"\n",
        )
        .unwrap();
        fs::write(root.join(".project/issues/issue-0001/body.md"), "Body\n").unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        root
    }

    fn write_mapping(root: &Path) {
        fs::write(
            root.join(".project/remotes/github/mappings/issues.toml"),
            "schema = \"allodium.github.issue-mappings/v0\"\n\n[issues.issue-0001]\nnumber = 17\nurl = \"https://github.com/owner/repo/issues/17\"\n",
        )
        .unwrap();
    }

    fn write_matching_observation(root: &Path) {
        fs::write(
            root.join(".project/remotes/github/observed/issues/issue-0001.toml"),
            "schema = \"allodium.github.observed-issue/v0\"\ncanonical_id = \"issue-0001\"\nnumber = 17\nstate = \"open\"\ntitle = \"Test issue\"\nurl = \"https://github.com/owner/repo/issues/17\"\nobserved_at = \"2026-09-16T12:00:00Z\"\n",
        )
        .unwrap();

        let issues = crate::load_issues(root).unwrap();
        fs::write(
            root.join(".project/remotes/github/observed/issues/issue-0001.body.md"),
            render_issue_body(&issues[0]),
        )
        .unwrap();
    }
}
