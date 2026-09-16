use crate::github::{GitHubOperation, GitHubPlan, PLAN_SCHEMA_V0};
use crate::load_remote;
use crate::wiki::{load_wiki, safe_relative_path};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const OBSERVED_WIKI_SCHEMA_V0: &str = "allodium.github.observed-wiki/v0";
pub type WikiFileSet = BTreeMap<String, Vec<u8>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedWiki {
    pub schema: String,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    pub observed_at: String,
}

pub fn desired_wiki_files(root: impl AsRef<Path>) -> Result<Option<WikiFileSet>, String> {
    let root = root.as_ref();
    let Some(wiki) = load_wiki(root)? else {
        return Ok(None);
    };
    let home = safe_relative_path(&wiki.record.home)?;
    if home.components().count() != 1 {
        return Err(format!(
            "GitHub wiki projection v0 requires the canonical home page to be top-level; {:?} is nested",
            wiki.record.home
        ));
    }

    let mut entries = fs::read_dir(&wiki.directory)
        .map_err(|error| format!("{}: {error}", wiki.directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", wiki.directory.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut files = WikiFileSet::new();
    for entry in entries {
        let path = entry.path();
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| format!("{}: wiki filename is not UTF-8", path.display()))?
            .to_owned();
        if name == "wiki.toml" {
            continue;
        }

        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "{}: GitHub wiki projection v0 does not follow symlinks",
                path.display()
            ));
        }
        if metadata.is_dir() {
            return Err(format!(
                "{}: GitHub wiki projection v0 supports top-level Markdown pages only; nested directories and assets require the future wiki asset strategy",
                path.display()
            ));
        }
        if !metadata.is_file() {
            return Err(format!(
                "{}: unsupported canonical wiki filesystem object",
                path.display()
            ));
        }
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            return Err(format!(
                "{}: GitHub wiki projection v0 supports Markdown pages only; assets are not yet projected",
                path.display()
            ));
        }
        validate_github_page_filename(&name)?;

        let canonical_relative = PathBuf::from(&name);
        let provider_name = if canonical_relative == home {
            "Home.md".to_owned()
        } else {
            name.clone()
        };
        if files.contains_key(&provider_name) {
            return Err(format!(
                "canonical wiki paths collide at GitHub Wiki page {:?}; choose a distinct home/page filename",
                provider_name
            ));
        }
        let body = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        std::str::from_utf8(&body).map_err(|error| {
            format!(
                "{}: GitHub wiki Markdown pages must be UTF-8 in projection v0: {error}",
                path.display()
            )
        })?;
        files.insert(provider_name, body);
    }

    if !files.contains_key("Home.md") {
        return Err(format!(
            "canonical wiki home {:?} did not resolve to a projected GitHub Wiki Home.md page",
            wiki.record.home
        ));
    }
    Ok(Some(files))
}

pub fn plan_wiki(root: impl AsRef<Path>, remote_name: &str) -> Result<GitHubPlan, String> {
    let root = root.as_ref();
    let remote = load_remote(root, remote_name)?;
    if remote.kind != "github" {
        return Err(format!(
            "remote {remote_name:?} has kind {:?}, not \"github\"",
            remote.kind
        ));
    }

    let Some(desired) = desired_wiki_files(root)? else {
        return Ok(GitHubPlan {
            schema: PLAN_SCHEMA_V0.into(),
            remote: remote_name.into(),
            repository: remote.repository,
            operations: Vec::new(),
        });
    };

    let mut operations = Vec::new();
    match load_observed_wiki(root, remote_name)? {
        None => operations.push(GitHubOperation {
            canonical_id: "wiki".into(),
            action: "observe_wiki".into(),
            number: None,
            fields: vec!["revision".into(), "files".into()],
            reason: "canonical wiki exists but no local GitHub Wiki observation is recorded".into(),
        }),
        Some((observation, observed_files)) if observation.head_sha.is_none() => {
            if !observed_files.is_empty() {
                return Err(
                    "observed GitHub Wiki has no HEAD revision but contains files; refuse inconsistent uninitialized-provider snapshot"
                        .into(),
                );
            }
            operations.push(GitHubOperation {
                canonical_id: "wiki".into(),
                action: "wiki_bootstrap_required".into(),
                number: None,
                fields: vec!["files".into()],
                reason: "GitHub Wiki is enabled but its separate wiki repository is not initialized; provider bootstrap is required before canonical pages can be projected"
                    .into(),
            });
        }
        Some((_observation, observed_files)) if observed_files != desired => {
            operations.push(GitHubOperation {
                canonical_id: "wiki".into(),
                action: "update_wiki".into(),
                number: None,
                fields: vec!["files".into()],
                reason: "canonical wiki files differ from the last observed GitHub Wiki tree"
                    .into(),
            });
        }
        Some(_) => {}
    }

    Ok(GitHubPlan {
        schema: PLAN_SCHEMA_V0.into(),
        remote: remote_name.into(),
        repository: remote.repository,
        operations,
    })
}

pub fn load_observed_wiki(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<Option<(ObservedWiki, WikiFileSet)>, String> {
    let directory = observed_wiki_directory(root.as_ref(), remote_name);
    let metadata_path = directory.join("wiki.toml");
    if !metadata_path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&metadata_path)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let observation: ObservedWiki =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    if observation.schema != OBSERVED_WIKI_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported schema {:?}; expected {:?}",
            metadata_path.display(),
            observation.schema,
            OBSERVED_WIKI_SCHEMA_V0
        ));
    }
    let files = read_file_tree(&directory.join("files"))?;
    Ok(Some((observation, files)))
}

pub fn write_observed_wiki_snapshot(
    root: impl AsRef<Path>,
    remote_name: &str,
    observation: &ObservedWiki,
    files: &WikiFileSet,
) -> Result<(), String> {
    if observation.schema != OBSERVED_WIKI_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed wiki schema {:?}",
            observation.schema
        ));
    }
    let directory = observed_wiki_directory(root.as_ref(), remote_name);
    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let text = toml::to_string_pretty(observation)
        .map_err(|error| format!("could not serialize GitHub Wiki observation: {error}"))?;
    fs::write(directory.join("wiki.toml"), text)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    write_file_tree(&directory.join("files"), files)
}

pub fn read_file_tree(directory: &Path) -> Result<WikiFileSet, String> {
    let mut files = WikiFileSet::new();
    if !directory.exists() {
        return Ok(files);
    }
    read_file_tree_into(directory, directory, &mut files)?;
    Ok(files)
}

pub fn write_file_tree(directory: &Path, files: &WikiFileSet) -> Result<(), String> {
    if directory.exists() {
        fs::remove_dir_all(directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
    }
    fs::create_dir_all(directory).map_err(|error| format!("{}: {error}", directory.display()))?;

    for (relative, body) in files {
        let relative = safe_snapshot_path(relative)?;
        let path = directory.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        fs::write(&path, body).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

fn observed_wiki_directory(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/wiki")
}

fn read_file_tree_into(
    base: &Path,
    directory: &Path,
    files: &mut WikiFileSet,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "{}: observed wiki snapshots may not contain symlinks",
                path.display()
            ));
        }
        if metadata.is_dir() {
            read_file_tree_into(base, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(format!(
                "{}: unsupported observed wiki filesystem object",
                path.display()
            ));
        }
        let relative = path.strip_prefix(base).map_err(|error| {
            format!(
                "{}: could not derive relative wiki path: {error}",
                path.display()
            )
        })?;
        let key = relative
            .to_str()
            .ok_or_else(|| format!("{}: wiki path is not UTF-8", path.display()))?
            .replace('\\', "/");
        files.insert(
            key,
            fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?,
        );
    }
    Ok(())
}

fn safe_snapshot_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() {
        return Err("observed wiki file path must not be empty".into());
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(format!(
            "observed wiki file path {value:?} must be relative"
        ));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!(
                "observed wiki file path {value:?} contains a non-normal component"
            ));
        }
    }
    Ok(path.to_path_buf())
}

fn validate_github_page_filename(name: &str) -> Result<(), String> {
    const FORBIDDEN: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    if name.chars().any(|character| FORBIDDEN.contains(&character)) {
        return Err(format!(
            "canonical wiki page {name:?} contains a filename character unsupported by the GitHub Wiki v0 projection"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn declared_home_projects_to_github_home() {
        let root = test_project("home-map", "index.md");
        fs::write(root.join(".project/wiki/index.md"), "# Index\n").unwrap();
        fs::write(
            root.join(".project/wiki/Architecture.md"),
            "# Architecture\n",
        )
        .unwrap();

        let files = desired_wiki_files(&root).unwrap().unwrap();
        assert_eq!(files.get("Home.md").unwrap(), b"# Index\n");
        assert_eq!(files.get("Architecture.md").unwrap(), b"# Architecture\n");
        assert!(!files.contains_key("index.md"));
        assert!(!files.contains_key("wiki.toml"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn assets_fail_loudly_in_v0() {
        let root = test_project("assets", "Home.md");
        fs::write(root.join(".project/wiki/Home.md"), "# Home\n").unwrap();
        fs::create_dir_all(root.join(".project/wiki/assets")).unwrap();
        fs::write(root.join(".project/wiki/assets/logo.png"), b"png").unwrap();

        let error = desired_wiki_files(&root).unwrap_err();
        assert!(error.contains("future wiki asset strategy"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_observation_plans_observe() {
        let root = test_project("observe", "Home.md");
        fs::write(root.join(".project/wiki/Home.md"), "# Home\n").unwrap();

        let plan = plan_wiki(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "observe_wiki");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uninitialized_provider_plans_non_mutating_bootstrap_requirement() {
        let root = test_project("bootstrap", "Home.md");
        fs::write(root.join(".project/wiki/Home.md"), "# Home\n").unwrap();
        let observation = ObservedWiki {
            schema: OBSERVED_WIKI_SCHEMA_V0.into(),
            branch: "master".into(),
            head_sha: None,
            observed_at: "2026-09-16T18:00:00Z".into(),
        };
        write_observed_wiki_snapshot(&root, "github", &observation, &WikiFileSet::new()).unwrap();

        let plan = plan_wiki(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "wiki_bootstrap_required");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matching_observation_is_idempotent() {
        let root = test_project("idempotent", "Home.md");
        fs::write(root.join(".project/wiki/Home.md"), "# Home\n").unwrap();
        let desired = desired_wiki_files(&root).unwrap().unwrap();
        let observation = ObservedWiki {
            schema: OBSERVED_WIKI_SCHEMA_V0.into(),
            branch: "master".into(),
            head_sha: Some("abc123".into()),
            observed_at: "2026-09-16T18:00:00Z".into(),
        };
        write_observed_wiki_snapshot(&root, "github", &observation, &desired).unwrap();

        let plan = plan_wiki(&root, "github").unwrap();
        assert!(plan.operations.is_empty(), "{:?}", plan.operations);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_canonical_page_plans_update() {
        let root = test_project("update", "Home.md");
        fs::write(root.join(".project/wiki/Home.md"), "# Home\n").unwrap();
        let observed_files = desired_wiki_files(&root).unwrap().unwrap();
        let observation = ObservedWiki {
            schema: OBSERVED_WIKI_SCHEMA_V0.into(),
            branch: "master".into(),
            head_sha: Some("abc123".into()),
            observed_at: "2026-09-16T18:00:00Z".into(),
        };
        write_observed_wiki_snapshot(&root, "github", &observation, &observed_files).unwrap();
        fs::write(root.join(".project/wiki/Home.md"), "# Changed\n").unwrap();

        let plan = plan_wiki(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "update_wiki");
        assert_eq!(plan.operations[0].fields, vec!["files"]);

        fs::remove_dir_all(root).unwrap();
    }

    fn test_project(name: &str, home: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-github-wiki-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".project/wiki")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::write(
            root.join(".project/wiki/wiki.toml"),
            format!("schema = \"allodium.wiki/v0\"\ntitle = \"Test Wiki\"\nhome = \"{home}\"\n"),
        )
        .unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        root
    }
}
