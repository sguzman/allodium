use super::{GitHubAdapter, now};
use allodium_core::board::load_boards;
use allodium_core::github_project::{
    OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0, ObservedProjectsCapabilities,
    ProjectsProjectionConfig, load_projects_projection_config,
    write_observed_projects_capabilities,
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
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejected_projects_credential_does_not_become_authority() {
        let root = test_root("rejected");
        write_config(&root, "ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED");
        unsafe {
            env::set_var(
                "ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED",
                "not-secret-in-test",
            );
        }
        let (base, handle) = one_response_server(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        );
        let adapter = test_adapter(base);
        let report = observe_projects_capabilities(&adapter, &root, "github").unwrap();
        handle.join().unwrap();
        unsafe {
            env::remove_var("ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED");
        }
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
        unsafe {
            env::set_var("ALLODIUM_TEST_PROJECTS_TOKEN_SUCCESS", "not-secret-in-test");
        }
        let body = r#"{"data":{"user":{"id":"U_kgDOtest","projectsV2":{"totalCount":0}}}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base, handle) = one_response_server(&response);
        let adapter = test_adapter(base);
        observe_projects_capabilities(&adapter, &root, "github").unwrap();
        handle.join().unwrap();
        unsafe {
            env::remove_var("ALLODIUM_TEST_PROJECTS_TOKEN_SUCCESS");
        }
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
