from pathlib import Path

GITHUB_PROJECT_RS = r'''use crate::board::load_boards;
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
            fields: vec!["owner".into(), "credential_env".into(), "board_bindings".into()],
            reason: "canonical boards exist but GitHub Projects projection configuration is missing"
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
            reason: "observed GitHub Projects authority does not match the explicitly configured owner"
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

fn validate_projection_config(path: &Path, config: &ProjectsProjectionConfig) -> Result<(), String> {
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
            return Err(format!("{}: board binding IDs must not be empty", path.display()));
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
        assert_eq!(plan.operations[0].fields, vec!["ALLODIUM_GITHUB_PROJECTS_TOKEN"]);
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
'''

PROJECT_RUNTIME_RS = r'''use super::{GitHubAdapter, now};
use allodium_core::board::load_boards;
use allodium_core::github_project::{
    OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0, ObservedProjectsCapabilities,
    ProjectsProjectionConfig, load_projects_projection_config, write_observed_projects_capabilities,
};
use serde_json::{Value, json};
use std::env;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ProjectsObserveReport {
    pub capabilities_observed: usize,
}

pub(super) fn observe_projects_capabilities(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<ProjectsObserveReport, String> {
    let boards = load_boards(root)?;
    if boards.is_empty() {
        return Ok(ProjectsObserveReport::default());
    }
    let Some(config) = load_projects_projection_config(root, remote_name)? else {
        return Ok(ProjectsObserveReport::default());
    };
    if !boards.iter().any(|board| {
        config
            .boards
            .get(&board.record.id)
            .is_some_and(|binding| binding.enabled)
    }) {
        return Ok(ProjectsObserveReport::default());
    }

    let token = env::var(&config.credential_env)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let (authenticated, owner_node_id) = match token.as_deref() {
        None => (false, None),
        Some(token) => match probe_projects_owner(adapter, &config, token)? {
            Some(owner_node_id) => (true, Some(owner_node_id)),
            None => (false, None),
        },
    };

    let observed = ObservedProjectsCapabilities {
        schema: OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0.into(),
        credential_available: token.is_some(),
        authenticated,
        owner_kind: config.owner_kind.clone(),
        owner: config.owner.clone(),
        owner_node_id,
        observed_at: now(),
    };
    write_observed_projects_capabilities(root, remote_name, &observed)?;
    Ok(ProjectsObserveReport {
        capabilities_observed: 1,
    })
}

fn probe_projects_owner(
    adapter: &GitHubAdapter,
    config: &ProjectsProjectionConfig,
    token: &str,
) -> Result<Option<String>, String> {
    let (owner_field, query) = match config.owner_kind.as_str() {
        "user" => (
            "user",
            "query($login: String!) { user(login: $login) { id projectsV2(first: 1) { totalCount } } }",
        ),
        "organization" => (
            "organization",
            "query($login: String!) { organization(login: $login) { id projectsV2(first: 1) { totalCount } } }",
        ),
        other => return Err(format!("unsupported GitHub Projects owner kind {other:?}")),
    };
    let response = adapter
        .client
        .post(adapter.api_url("/graphql"))
        .bearer_auth(token)
        .json(&json!({
            "query": query,
            "variables": { "login": config.owner },
        }))
        .send()
        .map_err(|error| format!("GitHub Projects capability probe failed: {error}"))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Ok(None);
    }
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "<unreadable response body>".into());
        return Err(format!(
            "GitHub Projects capability probe returned {status}: {body}"
        ));
    }

    let payload: Value = response
        .json()
        .map_err(|error| format!("invalid GitHub Projects GraphQL response: {error}"))?;
    if payload
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
    {
        return Ok(None);
    }
    Ok(payload
        .get("data")
        .and_then(|data| data.get(owner_field))
        .and_then(|owner| owner.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use allodium_core::github_project::load_observed_projects_capabilities;
    use reqwest::blocking::Client;
    use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
    use std::fs;
    use std::net::{TcpListener, TcpStream};
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejected_projects_credential_does_not_become_authority() {
        let root = test_root("rejected");
        write_config(&root, "ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED");
        unsafe { env::set_var("ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED", "not-secret-in-test"); }
        let (base, handle) = one_response_server(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        );
        let adapter = test_adapter(base);
        let report = observe_projects_capabilities(&adapter, &root, "github").unwrap();
        handle.join().unwrap();
        unsafe { env::remove_var("ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED"); }
        assert_eq!(report.capabilities_observed, 1);
        let observed = load_observed_projects_capabilities(&root, "github")
            .unwrap()
            .unwrap();
        assert!(observed.credential_available);
        assert!(!observed.authenticated);
        assert!(observed.owner_node_id.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn successful_projects_probe_records_only_owner_identity_not_token() {
        let root = test_root("success");
        write_config(&root, "ALLODIUM_TEST_PROJECTS_TOKEN_SUCCESS");
        unsafe { env::set_var("ALLODIUM_TEST_PROJECTS_TOKEN_SUCCESS", "not-secret-in-test"); }
        let body = r#"{"data":{"user":{"id":"U_kgDOtest","projectsV2":{"totalCount":0}}}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        let (base, handle) = one_response_server(&response);
        let adapter = test_adapter(base);
        observe_projects_capabilities(&adapter, &root, "github").unwrap();
        handle.join().unwrap();
        unsafe { env::remove_var("ALLODIUM_TEST_PROJECTS_TOKEN_SUCCESS"); }
        let path = root.join(".project/remotes/github/observed/projects/capabilities.toml");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("authenticated = true"));
        assert!(text.contains("owner_node_id = \"U_kgDOtest\""));
        assert!(!text.contains("not-secret-in-test"));
        fs::remove_dir_all(root).unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-projects-runtime-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".project/issues/issue-0001")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::create_dir_all(root.join(".project/boards/board-0001")).unwrap();
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
            root.join(".project/boards/board-0001/board.toml"),
            "schema = \"allodium.board/v0\"\nid = \"board-0001\"\ntitle = \"Board\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"sguzman/allodium\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        root
    }

    fn write_config(root: &Path, credential_env: &str) {
        fs::write(
            root.join(".project/remotes/github/projects.toml"),
            format!(
                "schema = \"allodium.github.projects-projection/v0\"\nowner_kind = \"user\"\nowner = \"sguzman\"\ncredential_env = \"{credential_env}\"\n\n[boards.board-0001]\nenabled = true\n"
            ),
        )
        .unwrap();
    }

    fn test_adapter(api_base: String) -> GitHubAdapter {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("allodium-test"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        GitHubAdapter {
            client: Client::builder().default_headers(headers).build().unwrap(),
            repository: "sguzman/allodium".into(),
            token: None,
            api_base,
        }
    }

    fn one_response_server(response: &str) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response = response.to_owned();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), handle)
    }

    fn read_request(stream: &mut TcpStream) {
        let mut buffer = [0_u8; 8192];
        let mut received = Vec::new();
        loop {
            let count = stream.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            received.extend_from_slice(&buffer[..count]);
            if received.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
    }
}
'''

CONFIG = '''schema = "allodium.github.projects-projection/v0"
owner_kind = "user"
owner = "sguzman"
credential_env = "ALLODIUM_GITHUB_PROJECTS_TOKEN"

[boards.board-0001]
enabled = true
'''


def replace_once(text, old, new, label):
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    return text.replace(old, new, 1)

Path("crates/allodium-core/src/github_project.rs").write_text(GITHUB_PROJECT_RS)
Path("crates/allodium-github/src/project.rs").write_text(PROJECT_RUNTIME_RS)
Path(".project/remotes/github/projects.toml").write_text(CONFIG)

core_lib = Path("crates/allodium-core/src/lib.rs")
text = core_lib.read_text()
text = replace_once(text, "pub mod github_milestone;\n", "pub mod github_milestone;\npub mod github_project;\n", "core project module")
core_lib.write_text(text)

cli = Path("crates/allodium-cli/src/main.rs")
text = cli.read_text()
anchor = '''        plan.operations.extend(discussion_plan.operations);
        Ok(plan)
'''
replacement = '''        plan.operations.extend(discussion_plan.operations);

        let projects_plan = allodium_core::github_project::plan_boards(&root, remote_name)?;
        if plan.remote != projects_plan.remote || plan.repository != projects_plan.repository {
            return Err(
                "GitHub Projects projection plan disagrees about the configured remote".into(),
            );
        }
        plan.operations.extend(projects_plan.operations);
        Ok(plan)
'''
text = replace_once(text, anchor, replacement, "CLI Projects plan integration")
anchor = '''            println!(
                "observed {} mapped GitHub Discussion(s)",
                report.discussions_observed
            );
'''
replacement = anchor + '''            println!(
                "observed {} GitHub Projects capability snapshot(s)",
                report.projects_capabilities_observed
            );
'''
text = replace_once(text, anchor, replacement, "CLI Projects observe output")
anchor = '''            println!(
                "{} GitHub Discussion projection(s) request unsupported category reclassification",
                report.discussion_category_change_unsupported
            );
'''
replacement = anchor + '''            println!(
                "observed {} GitHub Projects capability snapshot(s)",
                report.projects_capabilities_observed
            );
            println!(
                "{} GitHub Projects projection(s) require configuration",
                report.projects_config_required
            );
            println!(
                "{} GitHub Projects projection(s) require separate authorization",
                report.projects_auth_required
            );
            println!(
                "{} GitHub Projects projection(s) require owner review",
                report.projects_owner_review_required
            );
            println!(
                "{} GitHub Projects projection(s) are waiting for ProjectV2 runtime",
                report.projects_runtime_required
            );
'''
text = replace_once(text, anchor, replacement, "CLI Projects apply output")
cli.write_text(text)

lib = Path("crates/allodium-github/src/lib.rs")
text = lib.read_text()
text = replace_once(text, "mod milestone;\n", "mod milestone;\nmod project;\n", "GitHub Projects module")
text = replace_once(
    text,
    "    pub discussion_social_snapshots_archived: usize,\n",
    "    pub discussion_social_snapshots_archived: usize,\n    pub projects_capabilities_observed: usize,\n",
    "Projects observe counter",
)
text = replace_once(
    text,
    "    pub discussion_category_change_unsupported: usize,\n",
    "    pub discussion_category_change_unsupported: usize,\n    pub projects_capabilities_observed: usize,\n    pub projects_config_required: usize,\n    pub projects_auth_required: usize,\n    pub projects_owner_review_required: usize,\n    pub projects_runtime_required: usize,\n",
    "Projects apply counters",
)
anchor = '''        report.discussion_social_snapshots_archived += discussion_report.social_snapshots_archived;

        for issue in self.fetch_repository_issues()? {
'''
replacement = '''        report.discussion_social_snapshots_archived += discussion_report.social_snapshots_archived;

        let projects_report = project::observe_projects_capabilities(self, root, remote_name)?;
        report.projects_capabilities_observed += projects_report.capabilities_observed;

        for issue in self.fetch_repository_issues()? {
'''
text = replace_once(text, anchor, replacement, "Projects observe integration")
anchor = '''                "reopen_discussion" => {
                    let number = require_number(operation)?;
                    discussion::apply_reopen_discussion(
                        self,
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        number,
                    )?;
                    report.discussions_reopened += 1;
                }
'''
replacement = anchor + '''                "observe_projects_capabilities" => {
                    let projects = project::observe_projects_capabilities(self, root, &plan.remote)?;
                    report.projects_capabilities_observed += projects.capabilities_observed;
                }
                "projects_config_required" => {
                    report.projects_config_required += 1;
                }
                "projects_auth_required" => {
                    report.projects_auth_required += 1;
                }
                "projects_owner_review_required" => {
                    report.projects_owner_review_required += 1;
                }
                "projects_runtime_required" => {
                    report.projects_runtime_required += 1;
                }
'''
text = replace_once(text, anchor, replacement, "Projects apply integration")
lib.write_text(text)
