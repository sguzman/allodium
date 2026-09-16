use super::{GitHubAdapter, now};
use allodium_core::github_wiki::{
    OBSERVED_WIKI_SCHEMA_V0, ObservedWiki, WikiFileSet, desired_wiki_files, load_observed_wiki,
    write_file_tree, write_observed_wiki_snapshot,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const WIKI_REMOTE_CHANGE_SCHEMA_V0: &str = "allodium.github.wiki-remote-change-observation/v0";
const DEFAULT_WIKI_BRANCH: &str = "master";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct WikiObserveReport {
    pub observed: usize,
    pub remote_changes_archived: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiRepositoryCapabilities {
    has_wiki: bool,
    private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteWikiHead {
    branch: String,
    head_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteWikiSnapshot {
    branch: String,
    head_sha: Option<String>,
    files: WikiFileSet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct WikiRemoteChangeEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    head_sha: Option<String>,
}

pub(super) fn observe_wiki(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<WikiObserveReport, String> {
    let Some(desired) = desired_wiki_files(root)? else {
        return Ok(WikiObserveReport::default());
    };
    let previous = load_observed_wiki(root, remote_name)?;
    let live = fetch_remote_snapshot(adapter)?;
    let observed_at = now();
    let archived = archive_remote_change_if_needed(
        root,
        remote_name,
        previous.as_ref(),
        &live,
        &desired,
        &observed_at,
    )?;
    let observation = ObservedWiki {
        schema: OBSERVED_WIKI_SCHEMA_V0.into(),
        branch: live.branch,
        head_sha: live.head_sha,
        observed_at,
    };
    write_observed_wiki_snapshot(root, remote_name, &observation, &live.files)?;
    Ok(WikiObserveReport {
        observed: 1,
        remote_changes_archived: archived,
    })
}

pub(super) fn apply_wiki_update(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<(), String> {
    adapter.require_write_token()?;
    let desired = desired_wiki_files(root)?
        .ok_or_else(|| "refusing GitHub Wiki update: no canonical wiki exists".to_string())?;
    let (observed, _) = load_observed_wiki(root, remote_name)?.ok_or_else(|| {
"refusing GitHub Wiki update: no observed wiki revision; observe and re-plan before applying"
.to_string()
})?;
    let capabilities = repository_capabilities(adapter)?;
    require_wiki_enabled(&capabilities)?;
    let live_head = fetch_remote_head(adapter, &capabilities, true)?;
    assert_wiki_not_stale(&observed, &live_head)?;

    let temporary = TemporaryDirectory::new("apply")?;
    let worktree = temporary.path.join("wiki");
    if observed.head_sha.is_some() {
        clone_wiki(adapter, &capabilities, &live_head, &worktree, true)?;
        let cloned_head = git_stdout(
            adapter,
            Some(&worktree),
            &["rev-parse".into(), "HEAD".into()],
            "read cloned GitHub Wiki revision",
        )?;
        if cloned_head.trim() != observed.head_sha.as_deref().unwrap_or_default() {
            return Err(
"refusing stale GitHub Wiki update: remote changed while cloning; observe and re-plan before applying"
.into(),
);
        }
    } else {
        fs::create_dir_all(&worktree)
            .map_err(|error| format!("{}: {error}", worktree.display()))?;
        git_stdout(
            adapter,
            Some(&worktree),
            &["init".into()],
            "initialize temporary GitHub Wiki worktree",
        )?;
        git_stdout(
            adapter,
            Some(&worktree),
            &["checkout".into(), "-b".into(), observed.branch.clone()],
            "create initial GitHub Wiki branch",
        )?;
    }

    replace_worktree_files(&worktree, &desired)?;
    git_stdout(
        adapter,
        Some(&worktree),
        &["add".into(), "-A".into()],
        "stage GitHub Wiki projection",
    )?;
    let status = git_stdout(
        adapter,
        Some(&worktree),
        &["status".into(), "--porcelain".into()],
        "inspect GitHub Wiki projection changes",
    )?;
    if status.trim().is_empty() {
        observe_wiki(adapter, root, remote_name)?;
        return Ok(());
    }

    configure_commit_identity(adapter, &worktree)?;
    git_stdout(
        adapter,
        Some(&worktree),
        &[
            "commit".into(),
            "-m".into(),
            "allodium: reconcile canonical wiki".into(),
        ],
        "commit GitHub Wiki projection",
    )?;

    let authenticated = authenticated_wiki_url(adapter)?;
    let has_origin = git_output(
        Some(&worktree),
        &["remote".into(), "get-url".into(), "origin".into()],
    )?
    .status
    .success();
    if has_origin {
        git_stdout(
            adapter,
            Some(&worktree),
            &[
                "remote".into(),
                "set-url".into(),
                "origin".into(),
                authenticated.clone(),
            ],
            "configure temporary GitHub Wiki push remote",
        )?;
    } else {
        git_stdout(
            adapter,
            Some(&worktree),
            &[
                "remote".into(),
                "add".into(),
                "origin".into(),
                authenticated.clone(),
            ],
            "configure temporary GitHub Wiki push remote",
        )?;
    }

    let push = git_output(
        Some(&worktree),
        &[
            "push".into(),
            "origin".into(),
            format!("HEAD:refs/heads/{}", observed.branch),
        ],
    )?;
    if !push.status.success() {
        let error = sanitized_output(adapter, &push);
        if observed.head_sha.is_none() && looks_like_uninitialized_wiki(&error) {
            return Err(
"GitHub Wiki bootstrap boundary: the repository has Wiki enabled, but GitHub has not initialized its separate wiki Git repository and rejected the initial push. Canonical `.project/wiki/` state is unchanged. GitHub currently requires an initial wiki page before the wiki repository can be cloned; create that one provider bootstrap page, then observe and re-plan."
.into(),
);
        }
        if error.contains("non-fast-forward")
            || error.contains("fetch first")
            || error.contains("[rejected]")
        {
            return Err(
"refusing stale GitHub Wiki update: remote advanced during apply; observe and re-plan before applying"
.into(),
);
        }
        return Err(format!("GitHub Wiki push failed: {error}"));
    }

    observe_wiki(adapter, root, remote_name)?;
    Ok(())
}

fn repository_capabilities(adapter: &GitHubAdapter) -> Result<ApiRepositoryCapabilities, String> {
    adapter.get(&format!("/repos/{}", adapter.repository))
}

fn require_wiki_enabled(capabilities: &ApiRepositoryCapabilities) -> Result<(), String> {
    if !capabilities.has_wiki {
        return Err(
"GitHub capability blocked: repository Wiki is disabled; canonical `.project/wiki/` remains authoritative and no provider mutation was attempted"
.into(),
);
    }
    Ok(())
}

fn fetch_remote_snapshot(adapter: &GitHubAdapter) -> Result<RemoteWikiSnapshot, String> {
    let capabilities = repository_capabilities(adapter)?;
    require_wiki_enabled(&capabilities)?;
    let head = fetch_remote_head(adapter, &capabilities, false)?;
    let files = if head.head_sha.is_some() {
        let temporary = TemporaryDirectory::new("observe")?;
        let worktree = temporary.path.join("wiki");
        clone_wiki(adapter, &capabilities, &head, &worktree, false)?;
        read_tracked_files(adapter, &worktree)?
    } else {
        WikiFileSet::new()
    };
    Ok(RemoteWikiSnapshot {
        branch: head.branch,
        head_sha: head.head_sha,
        files,
    })
}

fn fetch_remote_head(
    adapter: &GitHubAdapter,
    capabilities: &ApiRepositoryCapabilities,
    for_write: bool,
) -> Result<RemoteWikiHead, String> {
    require_wiki_enabled(capabilities)?;
    let url = if for_write || capabilities.private {
        authenticated_wiki_url(adapter)?
    } else {
        public_wiki_url(adapter)
    };
    let output = git_output(
        None,
        &["ls-remote".into(), "--symref".into(), url, "HEAD".into()],
    )?;
    if output.status.success() {
        return parse_ls_remote(&String::from_utf8_lossy(&output.stdout));
    }
    let error = sanitized_output(adapter, &output);
    if looks_like_uninitialized_wiki(&error) {
        return Ok(RemoteWikiHead {
            branch: DEFAULT_WIKI_BRANCH.into(),
            head_sha: None,
        });
    }
    Err(format!(
        "could not observe GitHub Wiki Git repository: {error}"
    ))
}

fn clone_wiki(
    adapter: &GitHubAdapter,
    capabilities: &ApiRepositoryCapabilities,
    head: &RemoteWikiHead,
    worktree: &Path,
    for_write: bool,
) -> Result<(), String> {
    let url = if for_write || capabilities.private {
        authenticated_wiki_url(adapter)?
    } else {
        public_wiki_url(adapter)
    };
    git_stdout(
        adapter,
        None,
        &[
            "clone".into(),
            "--quiet".into(),
            "--depth".into(),
            "1".into(),
            "--branch".into(),
            head.branch.clone(),
            url,
            worktree.display().to_string(),
        ],
        "clone GitHub Wiki",
    )?;
    Ok(())
}

fn read_tracked_files(adapter: &GitHubAdapter, worktree: &Path) -> Result<WikiFileSet, String> {
    let output = git_bytes(
        adapter,
        Some(worktree),
        &["ls-files".into(), "-z".into()],
        "enumerate GitHub Wiki tracked files",
    )?;
    let mut files = WikiFileSet::new();
    for raw in output
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
    {
        let relative = std::str::from_utf8(raw)
            .map_err(|error| format!("GitHub Wiki contains a non-UTF-8 tracked path: {error}"))?;
        let safe = safe_relative_path(relative)?;
        let path = worktree.join(&safe);
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "{}: refusing to follow provider wiki symlink during observation",
                path.display()
            ));
        }
        if !metadata.is_file() {
            return Err(format!(
                "{}: tracked GitHub Wiki object is not an ordinary file",
                path.display()
            ));
        }
        files.insert(
            relative.replace('\\', "/"),
            fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?,
        );
    }
    Ok(files)
}

fn replace_worktree_files(worktree: &Path, desired: &WikiFileSet) -> Result<(), String> {
    for entry in
        fs::read_dir(worktree).map_err(|error| format!("{}: {error}", worktree.display()))?
    {
        let entry = entry.map_err(|error| format!("{}: {error}", worktree.display()))?;
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        } else {
            fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }

    for (relative, body) in desired {
        let safe = safe_relative_path(relative)?;
        let path = worktree.join(safe);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        fs::write(&path, body).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

fn configure_commit_identity(adapter: &GitHubAdapter, worktree: &Path) -> Result<(), String> {
    git_stdout(
        adapter,
        Some(worktree),
        &[
            "config".into(),
            "user.name".into(),
            "Allodium wiki projection".into(),
        ],
        "configure GitHub Wiki commit author",
    )?;
    git_stdout(
        adapter,
        Some(worktree),
        &[
            "config".into(),
            "user.email".into(),
            "actions@users.noreply.github.com".into(),
        ],
        "configure GitHub Wiki commit email",
    )?;
    Ok(())
}

fn assert_wiki_not_stale(observed: &ObservedWiki, live: &RemoteWikiHead) -> Result<(), String> {
    if observed.branch != live.branch || observed.head_sha != live.head_sha {
        return Err(
"refusing stale GitHub Wiki update: remote changed after the recorded observation; observe and re-plan before applying"
.into(),
);
    }
    Ok(())
}

fn parse_ls_remote(text: &str) -> Result<RemoteWikiHead, String> {
    let mut branch = None;
    let mut head_sha = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("ref: refs/heads/") {
            if let Some((name, target)) = rest.split_once('\t') {
                if target == "HEAD" {
                    branch = Some(name.to_owned());
                }
            }
            continue;
        }
        if let Some((sha, target)) = line.split_once('\t') {
            if target == "HEAD" && !sha.trim().is_empty() {
                head_sha = Some(sha.to_owned());
            }
        }
    }
    Ok(RemoteWikiHead {
        branch: branch.unwrap_or_else(|| DEFAULT_WIKI_BRANCH.into()),
        head_sha,
    })
}

fn archive_remote_change_if_needed(
    root: &Path,
    remote_name: &str,
    previous: Option<&(ObservedWiki, WikiFileSet)>,
    live: &RemoteWikiSnapshot,
    desired: &WikiFileSet,
    observed_at: &str,
) -> Result<usize, String> {
    let changed = match previous {
        None => live.head_sha.is_some() || !live.files.is_empty(),
        Some((observation, files)) => observation.head_sha != live.head_sha || files != &live.files,
    };
    if !changed || live.files == *desired {
        return Ok(0);
    }

    let previous_head = previous.and_then(|(observation, _)| observation.head_sha.clone());
    let revision = live
        .head_sha
        .clone()
        .or_else(|| {
            previous_head
                .as_ref()
                .map(|head| format!("no-head-after-{head}"))
        })
        .unwrap_or_else(|| "no-head".into());
    let id = format!("github-wiki-remote-tree-{revision}");
    let directory = incoming_directory(root, remote_name, observed_at, &id)?;
    if directory.exists() {
        return Ok(0);
    }
    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let event = WikiRemoteChangeEvent {
schema: WIKI_REMOTE_CHANGE_SCHEMA_V0.into(),
id,
remote: remote_name.into(),
kind: "wiki.remote_tree.observed_change".into(),
observed_at: observed_at.into(),
evidence: "The GitHub Wiki Git tree changed outside canonical `.project/wiki/` state. Allodium archives provider-scoped before/after evidence and does not promote the change into canonical wiki files.".into(),
branch: live.branch.clone(),
previous_head_sha: previous_head,
head_sha: live.head_sha.clone(),
};
    let event_text = toml::to_string_pretty(&event)
        .map_err(|error| format!("could not serialize GitHub Wiki ingress event: {error}"))?;
    fs::write(directory.join("event.toml"), event_text)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    let before = previous.map(|(_, files)| files.clone()).unwrap_or_default();
    write_file_tree(&directory.join("before"), &before)?;
    write_file_tree(&directory.join("after"), &live.files)?;
    Ok(1)
}

fn incoming_directory(
    root: &Path,
    remote_name: &str,
    observed_at: &str,
    id: &str,
) -> Result<PathBuf, String> {
    let year = observed_at
        .get(0..4)
        .ok_or_else(|| format!("invalid observed_at timestamp {observed_at:?}"))?;
    let month = observed_at
        .get(5..7)
        .ok_or_else(|| format!("invalid observed_at timestamp {observed_at:?}"))?;
    Ok(root
        .join(".project/remotes")
        .join(remote_name)
        .join("incoming")
        .join(year)
        .join(month)
        .join(id))
}

fn public_wiki_url(adapter: &GitHubAdapter) -> String {
    format!("https://github.com/{}.wiki.git", adapter.repository)
}

fn authenticated_wiki_url(adapter: &GitHubAdapter) -> Result<String, String> {
    let token = adapter.token.as_ref().ok_or_else(|| {
        "GitHub Wiki mutation requires ALLODIUM_GITHUB_TOKEN, GH_TOKEN, or GITHUB_TOKEN".to_string()
    })?;
    Ok(format!(
        "https://x-access-token:{token}@github.com/{}.wiki.git",
        adapter.repository
    ))
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() {
        return Err("GitHub Wiki path must not be empty".into());
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(format!("GitHub Wiki path {value:?} must be relative"));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!(
                "GitHub Wiki path {value:?} contains a non-normal component"
            ));
        }
    }
    Ok(path.to_path_buf())
}

fn looks_like_uninitialized_wiki(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("repository not found")
        || lower.contains("does not appear to be a git repository")
        || lower.contains("not found") && lower.contains("wiki.git")
}

fn git_stdout(
    adapter: &GitHubAdapter,
    cwd: Option<&Path>,
    args: &[String],
    context: &str,
) -> Result<String, String> {
    let output = git_output(cwd, args)?;
    if !output.status.success() {
        return Err(format!("{context}: {}", sanitized_output(adapter, &output)));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{context}: git output is not UTF-8: {error}"))
}

fn git_bytes(
    adapter: &GitHubAdapter,
    cwd: Option<&Path>,
    args: &[String],
    context: &str,
) -> Result<Vec<u8>, String> {
    let output = git_output(cwd, args)?;
    if !output.status.success() {
        return Err(format!("{context}: {}", sanitized_output(adapter, &output)));
    }
    Ok(output.stdout)
}

fn git_output(cwd: Option<&Path>, args: &[String]) -> Result<Output, String> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
.env("GIT_TERMINAL_PROMPT", "0")
.output()
.map_err(|error| {
format!(
"could not execute Git for GitHub Wiki projection: {error}. Git is transport here, not authoritative state, but the GitHub Wiki adapter requires the `git` executable in PATH"
)
})
}

fn sanitized_output(adapter: &GitHubAdapter, output: &Output) -> String {
    let mut text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(token) = &adapter.token {
        text = text.replace(token, "***");
    }
    text.trim().to_owned()
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new(purpose: &str) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock error: {error}"))?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "allodium-github-wiki-{purpose}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wiki_symref_and_head() {
        let parsed =
            parse_ls_remote("ref: refs/heads/master\tHEAD\n0123456789abcdef\tHEAD\n").unwrap();
        assert_eq!(parsed.branch, "master");
        assert_eq!(parsed.head_sha.as_deref(), Some("0123456789abcdef"));
    }

    #[test]
    fn stale_guard_rejects_changed_head() {
        let observed = ObservedWiki {
            schema: OBSERVED_WIKI_SCHEMA_V0.into(),
            branch: "master".into(),
            head_sha: Some("old".into()),
            observed_at: "2026-09-16T18:00:00Z".into(),
        };
        let live = RemoteWikiHead {
            branch: "master".into(),
            head_sha: Some("new".into()),
        };
        let error = assert_wiki_not_stale(&observed, &live).unwrap_err();
        assert!(error.contains("stale GitHub Wiki update"));
    }

    #[test]
    fn remote_change_archive_is_idempotent() {
        let temporary = TemporaryDirectory::new("archive-test").unwrap();
        let root = &temporary.path;
        let previous_observation = ObservedWiki {
            schema: OBSERVED_WIKI_SCHEMA_V0.into(),
            branch: "master".into(),
            head_sha: Some("old".into()),
            observed_at: "2026-09-16T18:00:00Z".into(),
        };
        let previous_files = WikiFileSet::from([("Home.md".into(), b"old\n".to_vec())]);
        let previous = (previous_observation, previous_files);
        let live = RemoteWikiSnapshot {
            branch: "master".into(),
            head_sha: Some("new".into()),
            files: WikiFileSet::from([("Home.md".into(), b"foreign\n".to_vec())]),
        };
        let desired = WikiFileSet::from([("Home.md".into(), b"canonical\n".to_vec())]);
        let first = archive_remote_change_if_needed(
            root,
            "github",
            Some(&previous),
            &live,
            &desired,
            "2026-09-16T18:10:00Z",
        )
        .unwrap();
        let second = archive_remote_change_if_needed(
            root,
            "github",
            Some(&previous),
            &live,
            &desired,
            "2026-09-16T18:11:00Z",
        )
        .unwrap();
        assert_eq!(first, 1);
        assert_eq!(second, 0);
    }
}
