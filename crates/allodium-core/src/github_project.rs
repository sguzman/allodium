use crate::board::load_boards;
use crate::github::{GitHubOperation, GitHubPlan, PLAN_SCHEMA_V0};
use crate::load_remote;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECTS_PROJECTION_SCHEMA_V0: &str = "allodium.github.projects-projection/v0";
pub const OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0: &str =
    "allodium.github.observed-projects-capabilities/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectBoardBinding {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectsProjectionConfig {
    pub schema: String,
    pub owner_kind: String,
    pub owner: String,
    pub credential_env: String,
    #[serde(default)]
    pub boards: BTreeMap<String, ProjectBoardBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProjectsCapabilities {
    pub schema: String,
    pub credential_available: bool,
    pub authenticated: bool,
    pub owner_kind: String,
    pub owner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_node_id: Option<String>,
    pub observed_at: String,
}

pub fn plan_boards(root: impl AsRef<Path>, remote_name: &str) -> Result<GitHubPlan, String> {
    let root = root.as_ref();
    let remote = load_remote(root, remote_name)?;
    if remote.kind != "github" {
        return Err(format!(
            "remote {remote_name:?} has kind {:?}, not \"github\"",
            remote.kind
        ));
    }

    let boards = load_boards(root)?;
    let mut operations = Vec::new();
    if boards.is_empty() {
        return Ok(plan(remote_name, remote.repository, operations));
    }

    let Some(config) = load_projects_projection_config(root, remote_name)? else {
        operations.push(GitHubOperation {
            canonical_id: "boards".into(),
            action: "projects_config_required".into(),
            number: None,
            fields: vec![
                "owner".into(),
                "credential_env".into(),
                "board_bindings".into(),
            ],
            reason:
                "canonical boards exist but GitHub Projects projection configuration is missing"
                    .into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    };

    let mut missing_binding = false;
    for board in &boards {
        if !config.boards.contains_key(&board.record.id) {
            missing_binding = true;
            operations.push(GitHubOperation {
                canonical_id: board.record.id.clone(),
                action: "projects_config_required".into(),
                number: None,
                fields: vec!["board_binding".into()],
                reason: "canonical board has no explicit GitHub Projects projection binding".into(),
            });
        }
    }
    if missing_binding {
        return Ok(plan(remote_name, remote.repository, operations));
    }

    let enabled = boards
        .iter()
        .filter(|board| {
            config
                .boards
                .get(&board.record.id)
                .is_some_and(|binding| binding.enabled)
        })
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return Ok(plan(remote_name, remote.repository, operations));
    }

    let Some(capabilities) = load_observed_projects_capabilities(root, remote_name)? else {
        operations.push(GitHubOperation {
            canonical_id: "projects".into(),
            action: "observe_projects_capabilities".into(),
            number: None,
            fields: vec!["credential".into(), "owner".into()],
            reason: "GitHub Projects capability observation is missing".into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    };

    if !capabilities.credential_available || !capabilities.authenticated {
        operations.push(GitHubOperation {
            canonical_id: "projects".into(),
            action: "projects_auth_required".into(),
            number: None,
            fields: vec![config.credential_env.clone()],
            reason: format!(
                "GitHub Projects requires separately authorized credentials via {}; repository GITHUB_TOKEN authority is intentionally insufficient",
                config.credential_env
            ),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    }

    if capabilities.owner_kind != config.owner_kind
        || capabilities.owner != config.owner
        || capabilities.owner_node_id.is_none()
    {
        operations.push(GitHubOperation {
            canonical_id: "projects".into(),
            action: "projects_owner_review_required".into(),
            number: None,
            fields: vec!["owner_kind".into(), "owner".into(), "owner_node_id".into()],
            reason:
                "observed GitHub Projects authority does not match the explicitly configured owner"
                    .into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    }

    for board in enabled {
        operations.push(GitHubOperation {
            canonical_id: board.record.id.clone(),
            action: "projects_runtime_required".into(),
            number: None,
            fields: Vec::new(),
            reason: "GitHub Projects authority is available, but ProjectV2 mapping/mutation runtime is not enabled in this increment"
                .into(),
        });
    }

    Ok(plan(remote_name, remote.repository, operations))
}

pub fn load_projects_projection_config(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<Option<ProjectsProjectionConfig>, String> {
    let path = projects_projection_config_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let config: ProjectsProjectionConfig =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_projection_config(&path, &config)?;
    Ok(Some(config))
}

pub fn load_observed_projects_capabilities(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<Option<ObservedProjectsCapabilities>, String> {
    let path = observed_projects_capabilities_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedProjectsCapabilities =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported observed GitHub Projects capability schema {:?}; expected {:?}",
            path.display(),
            observed.schema,
            OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0
        ));
    }
    Ok(Some(observed))
}

pub fn write_observed_projects_capabilities(
    root: impl AsRef<Path>,
    remote_name: &str,
    observed: &ObservedProjectsCapabilities,
) -> Result<(), String> {
    if observed.schema != OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed GitHub Projects capability schema {:?}",
            observed.schema
        ));
    }
    let path = observed_projects_capabilities_path(root.as_ref(), remote_name);
    fs::create_dir_all(path.parent().expect("Projects capability path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(observed)
        .map_err(|error| format!("could not serialize GitHub Projects capabilities: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

fn validate_projection_config(
    path: &Path,
    config: &ProjectsProjectionConfig,
) -> Result<(), String> {
    if config.schema != PROJECTS_PROJECTION_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported GitHub Projects projection schema {:?}; expected {:?}",
            path.display(),
            config.schema,
            PROJECTS_PROJECTION_SCHEMA_V0
        ));
    }
    if !matches!(config.owner_kind.as_str(), "user" | "organization") {
        return Err(format!(
            "{}: owner_kind must be user or organization",
            path.display()
        ));
    }
    if config.owner.trim().is_empty() || config.credential_env.trim().is_empty() {
        return Err(format!(
            "{}: owner and credential_env must not be empty",
            path.display()
        ));
    }
    for board_id in config.boards.keys() {
        if board_id.trim().is_empty() {
            return Err(format!(
                "{}: board binding IDs must not be empty",
                path.display()
            ));
        }
    }
    Ok(())
}

fn plan(remote_name: &str, repository: String, operations: Vec<GitHubOperation>) -> GitHubPlan {
    GitHubPlan {
        schema: PLAN_SCHEMA_V0.into(),
        remote: remote_name.into(),
        repository,
        operations,
    }
}

fn projects_projection_config_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("projects.toml")
}

fn observed_projects_capabilities_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/projects/capabilities.toml")
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_config_is_non_mutating_requirement() {
        let root = test_root("missing-config");
        write_board(&root);
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "projects_config_required");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_capability_observation_plans_observation() {
        let root = test_root("observe");
        write_board(&root);
        write_config(&root);
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "observe_projects_capabilities");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn absent_projects_credential_is_explicit_requirement() {
        let root = test_root("auth");
        write_board(&root);
        write_config(&root);
        write_capability(&root, false, false, "user", "sguzman", None);
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "projects_auth_required");
        assert_eq!(
            plan.operations[0].fields,
            vec!["ALLODIUM_GITHUB_PROJECTS_TOKEN"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn observed_owner_mismatch_requires_review() {
        let root = test_root("owner");
        write_board(&root);
        write_config(&root);
        write_capability(&root, true, true, "user", "someone-else", Some("U_1"));
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "projects_owner_review_required");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_authority_stops_at_explicit_runtime_boundary() {
        let root = test_root("runtime");
        write_board(&root);
        write_config(&root);
        write_capability(&root, true, true, "user", "sguzman", Some("U_1"));
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].canonical_id, "board-0001");
        assert_eq!(plan.operations[0].action, "projects_runtime_required");
        fs::remove_dir_all(root).unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-github-projects-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".project/issues/issue-0001")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
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
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"sguzman/allodium\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        root
    }

    fn write_board(root: &Path) {
        let directory = root.join(".project/boards/board-0001");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("board.toml"),
            "schema = \"allodium.board/v0\"\nid = \"board-0001\"\ntitle = \"Test board\"\n",
        )
        .unwrap();
    }

    fn write_config(root: &Path) {
        fs::write(
            root.join(".project/remotes/github/projects.toml"),
            "schema = \"allodium.github.projects-projection/v0\"\nowner_kind = \"user\"\nowner = \"sguzman\"\ncredential_env = \"ALLODIUM_GITHUB_PROJECTS_TOKEN\"\n\n[boards.board-0001]\nenabled = true\n",
        )
        .unwrap();
    }

    fn write_capability(
        root: &Path,
        credential_available: bool,
        authenticated: bool,
        owner_kind: &str,
        owner: &str,
        owner_node_id: Option<&str>,
    ) {
        let path = root.join(".project/remotes/github/observed/projects/capabilities.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let owner_node_id = owner_node_id
            .map(|value| format!("owner_node_id = \"{value}\"\n"))
            .unwrap_or_default();
        fs::write(
            path,
            format!(
                "schema = \"allodium.github.observed-projects-capabilities/v0\"\ncredential_available = {credential_available}\nauthenticated = {authenticated}\nowner_kind = \"{owner_kind}\"\nowner = \"{owner}\"\n{owner_node_id}observed_at = \"2026-09-17T00:00:00Z\"\n"
            ),
        )
        .unwrap();
    }
}
