use super::{GitHubAdapter, now};
use allodium_core::discussion::load_discussions;
use allodium_core::github_discussion::{
    OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0, ObservedDiscussionCapabilities,
    ObservedDiscussionCategory, write_observed_discussion_capabilities,
};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct DiscussionObserveReport {
    pub capabilities_observed: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiRepositoryCapabilities {
    node_id: String,
    has_discussions: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphEnvelope {
    data: Option<GraphData>,
    #[serde(default)]
    errors: Vec<GraphError>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphError {
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphData {
    repository: Option<GraphRepository>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphRepository {
    #[serde(rename = "discussionCategories")]
    discussion_categories: GraphCategoryConnection,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCategoryConnection {
    nodes: Vec<GraphCategory>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCategory {
    id: String,
    name: String,
    slug: String,
    #[serde(rename = "isAnswerable")]
    is_answerable: bool,
}

pub(super) fn observe_discussions(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<DiscussionObserveReport, String> {
    if load_discussions(root)?.is_empty() {
        return Ok(DiscussionObserveReport::default());
    }
    observe_capabilities(adapter, root, remote_name)?;
    Ok(DiscussionObserveReport {
        capabilities_observed: 1,
    })
}

pub(super) fn apply_observe_discussion_capabilities(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<(), String> {
    observe_capabilities(adapter, root, remote_name).map(|_| ())
}

fn observe_capabilities(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<ObservedDiscussionCapabilities, String> {
    let repository: ApiRepositoryCapabilities =
        adapter.get(&format!("/repos/{}", adapter.repository))?;
    let categories = if repository.has_discussions {
        fetch_categories(adapter)?
    } else {
        Vec::new()
    };
    let observed = ObservedDiscussionCapabilities {
        schema: OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0.into(),
        enabled: repository.has_discussions,
        repository_node_id: repository.node_id,
        categories,
        observed_at: now(),
    };
    write_observed_discussion_capabilities(root, remote_name, &observed)?;
    Ok(observed)
}

fn fetch_categories(adapter: &GitHubAdapter) -> Result<Vec<ObservedDiscussionCategory>, String> {
    let (owner, name) = adapter.repository.split_once('/').ok_or_else(|| {
        format!(
            "GitHub repository {:?} is not in owner/name form",
            adapter.repository
        )
    })?;
    let query = r#"
query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    discussionCategories(first: 100) {
      nodes {
        id
        name
        slug
        isAnswerable
      }
    }
  }
}
"#;
    let envelope: GraphEnvelope = adapter.post(
        "/graphql",
        &json!({
            "query": query,
            "variables": {
                "owner": owner,
                "name": name,
            }
        }),
    )?;
    if !envelope.errors.is_empty() {
        let messages = envelope
            .errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "GitHub GraphQL Discussion-category query failed: {messages}"
        ));
    }
    let mut categories = envelope
        .data
        .and_then(|data| data.repository)
        .map(|repository| repository.discussion_categories.nodes)
        .ok_or_else(|| {
            format!(
                "GitHub GraphQL returned no repository while reading Discussion categories for {}",
                adapter.repository
            )
        })?
        .into_iter()
        .map(|category| ObservedDiscussionCategory {
            node_id: category.id,
            name: category.name,
            slug: category.slug,
            is_answerable: category.is_answerable,
        })
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(categories)
}

#[cfg(test)]
mod tests {
    use super::*;
    use allodium_core::github_discussion::load_observed_discussion_capabilities;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn disabled_repository_observation_never_queries_graphql() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&requests);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 8192];
            let count = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]);
            seen.lock()
                .unwrap()
                .push(request.lines().next().unwrap_or_default().to_string());
            let body = r#"{"node_id":"R_repo","has_discussions":false}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let root = test_root("disabled");
        write_canonical_discussion(&root);
        let adapter = test_adapter(&format!("http://{address}"));
        let report = observe_discussions(&adapter, &root, "github").unwrap();
        assert_eq!(report.capabilities_observed, 1);
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /repos/owner/repo "));
        let observed = load_observed_discussion_capabilities(&root, "github")
            .unwrap()
            .unwrap();
        assert!(!observed.enabled);
        assert_eq!(observed.repository_node_id, "R_repo");
        assert!(observed.categories.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enabled_repository_categories_are_sorted_and_persisted() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for response_body in [
                r#"{"node_id":"R_repo","has_discussions":true}"#,
                r#"{"data":{"repository":{"discussionCategories":{"nodes":[{"id":"DIC_z","name":"Zeta","slug":"zeta","isAnswerable":true},{"id":"DIC_a","name":"Alpha","slug":"alpha","isAnswerable":false}]}}}}"#,
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 16384];
                let _ = stream.read(&mut buffer).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                )
                .unwrap();
            }
        });

        let root = test_root("enabled");
        write_canonical_discussion(&root);
        let adapter = test_adapter(&format!("http://{address}"));
        observe_discussions(&adapter, &root, "github").unwrap();
        server.join().unwrap();
        let observed = load_observed_discussion_capabilities(&root, "github")
            .unwrap()
            .unwrap();
        assert!(observed.enabled);
        assert_eq!(
            observed
                .categories
                .iter()
                .map(|category| category.slug.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "zeta"]
        );
        assert!(!observed.categories[0].is_answerable);
        assert!(observed.categories[1].is_answerable);
        fs::remove_dir_all(root).unwrap();
    }

    fn write_canonical_discussion(root: &Path) {
        let directory = root.join(".project/discussions/discussion-0001");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("discussion.toml"),
            "schema = \"allodium.discussion/v0\"\nid = \"discussion-0001\"\ntitle = \"Test\"\nstate = \"open\"\n",
        )
        .unwrap();
        fs::write(directory.join("body.md"), "Body\n").unwrap();
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-discussion-runtime-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn test_adapter(api_base: &str) -> GitHubAdapter {
        GitHubAdapter {
            client: reqwest::blocking::Client::builder().build().unwrap(),
            repository: "owner/repo".into(),
            token: Some("test-token".into()),
            api_base: api_base.into(),
        }
    }
}
