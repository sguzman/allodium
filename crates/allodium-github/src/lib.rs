use allodium_core::github::{
    ArchiveOutcome, GitHubPlan, INCOMING_EVENT_SCHEMA_V0, ISSUE_MAPPINGS_SCHEMA_V0, IncomingActor,
    IncomingEvent, IncomingIssueComment, IncomingSource, IncomingTarget, IssueMapping,
    IssueMappings, OBSERVED_ISSUE_SCHEMA_V0, ObservedIssue, PLAN_SCHEMA_V0, render_issue_body,
};
use allodium_core::{CanonicalIssue, load_issues, load_remote};
use chrono::Utc;
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const API_BASE: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const OBSERVED_REVISION_SCHEMA_V0: &str = "allodium.github.observed-revision/v0";
const OBSERVED_COMMENT_SCHEMA_V0: &str = "allodium.github.observed-comment/v0";
const MANAGED_CHANGE_SCHEMA_V0: &str = "allodium.github.managed-change-observation/v0";

#[derive(Debug, Clone)]
pub struct GitHubAdapter {
    client: Client,
    repository: String,
    token: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObserveReport {
    pub issues_observed: usize,
    pub comments_archived: usize,
    pub comment_edits_archived: usize,
    pub managed_changes_archived: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub issues_created: usize,
    pub issues_updated: usize,
    pub issues_observed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ObservedRevision {
    schema: String,
    canonical_id: String,
    number: u64,
    remote_updated_at: String,
    observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ObservedComment {
    schema: String,
    canonical_id: String,
    issue_number: u64,
    comment_id: u64,
    remote_updated_at: String,
    url: String,
    actor_remote_id: String,
    actor_login: String,
    observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ManagedChangeEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    fields: Vec<String>,
    target: IncomingTarget,
    source: ManagedChangeSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ManagedChangeSource {
    remote_object_type: String,
    remote_object_id: String,
    url: String,
    remote_updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiIssue {
    number: u64,
    html_url: String,
    title: String,
    body: Option<String>,
    state: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiComment {
    id: u64,
    html_url: String,
    body: Option<String>,
    user: ApiUser,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiUser {
    id: u64,
    login: String,
}

impl GitHubAdapter {
    pub fn from_project(root: impl AsRef<Path>, remote_name: &str) -> Result<Self, String> {
        let remote = load_remote(root, remote_name)?;
        if remote.kind != "github" {
            return Err(format!(
                "remote {remote_name:?} has kind {:?}, not \"github\"",
                remote.kind
            ));
        }

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("allodium/0.0.1"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static(API_VERSION),
        );
        let client = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|error| format!("could not build GitHub HTTP client: {error}"))?;

        Ok(Self {
            client,
            repository: remote.repository,
            token: github_token_from_environment(),
        })
    }

    pub fn observe(
        &self,
        root: impl AsRef<Path>,
        remote_name: &str,
    ) -> Result<ObserveReport, String> {
        let root = root.as_ref();
        let mappings = load_mappings(root, remote_name)?;
        let mut report = ObserveReport::default();

        for (canonical_id, mapping) in mappings.issues {
            let observed_at = now();
            let issue = self.fetch_issue(mapping.number)?;
            report.managed_changes_archived += archive_managed_change_if_needed(
                root,
                remote_name,
                &canonical_id,
                &issue,
                &observed_at,
            )?;
            write_observed_issue(root, remote_name, &canonical_id, &issue, &observed_at)?;
            report.issues_observed += 1;

            for comment in self.fetch_issue_comments(mapping.number)? {
                match archive_comment_observation(
                    root,
                    remote_name,
                    &canonical_id,
                    mapping.number,
                    &comment,
                    &now(),
                )? {
                    CommentArchive::Created => report.comments_archived += 1,
                    CommentArchive::Edited => report.comment_edits_archived += 1,
                    CommentArchive::Unchanged => {}
                }
            }
        }

        Ok(report)
    }

    pub fn apply(&self, root: impl AsRef<Path>, plan: &GitHubPlan) -> Result<ApplyReport, String> {
        let root = root.as_ref();
        if plan.schema != PLAN_SCHEMA_V0 {
            return Err(format!(
                "unsupported plan schema {:?}; expected {:?}",
                plan.schema, PLAN_SCHEMA_V0
            ));
        }
        let remote = load_remote(root, &plan.remote)?;
        if remote.kind != "github" {
            return Err(format!("plan remote {:?} is not GitHub", plan.remote));
        }
        if remote.repository != plan.repository || self.repository != plan.repository {
            return Err(format!(
                "plan repository {:?} does not match configured repository {:?}",
                plan.repository, remote.repository
            ));
        }

        let issues = load_issues(root)?
            .into_iter()
            .map(|issue| (issue.record.id.clone(), issue))
            .collect::<BTreeMap<_, _>>();
        let mut mappings = load_mappings(root, &plan.remote)?;
        let mut report = ApplyReport::default();

        for operation in &plan.operations {
            let issue = issues.get(&operation.canonical_id).ok_or_else(|| {
                format!(
                    "plan references missing canonical issue {:?}",
                    operation.canonical_id
                )
            })?;

            match operation.action.as_str() {
                "create_issue" => {
                    self.require_write_token()?;
                    if mappings.issues.contains_key(&operation.canonical_id) {
                        return Err(format!(
                            "refusing create: {:?} already has a GitHub mapping",
                            operation.canonical_id
                        ));
                    }
                    let mut created = self.create_issue(issue)?;
                    if issue.record.state == "closed" {
                        created =
                            self.patch_issue(created.number, &json!({ "state": "closed" }))?;
                    }
                    mappings.issues.insert(
                        operation.canonical_id.clone(),
                        IssueMapping {
                            number: created.number,
                            url: created.html_url.clone(),
                        },
                    );
                    save_mappings(root, &plan.remote, &mappings)?;
                    write_observed_issue(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        &created,
                        &now(),
                    )?;
                    report.issues_created += 1;
                }
                "observe_issue" => {
                    let number = require_number(operation)?;
                    let live = self.fetch_issue(number)?;
                    archive_managed_change_if_needed(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        &live,
                        &now(),
                    )?;
                    write_observed_issue(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        &live,
                        &now(),
                    )?;
                    report.issues_observed += 1;
                }
                "update_issue" => {
                    self.require_write_token()?;
                    let number = require_number(operation)?;
                    self.assert_not_stale(root, &plan.remote, &operation.canonical_id, number)?;
                    let payload = update_payload(issue, &operation.fields);
                    let updated = self.patch_issue(number, &payload)?;
                    write_observed_issue(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        &updated,
                        &now(),
                    )?;
                    report.issues_updated += 1;
                }
                other => return Err(format!("unsupported GitHub plan operation {other:?}")),
            }
        }

        Ok(report)
    }

    fn fetch_issue(&self, number: u64) -> Result<ApiIssue, String> {
        self.get(&format!("/repos/{}/issues/{number}", self.repository))
    }

    fn fetch_issue_comments(&self, number: u64) -> Result<Vec<ApiComment>, String> {
        let mut page = 1;
        let mut comments = Vec::new();
        loop {
            let batch: Vec<ApiComment> = self.get(&format!(
                "/repos/{}/issues/{number}/comments?per_page=100&page={page}",
                self.repository
            ))?;
            let count = batch.len();
            comments.extend(batch);
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(comments)
    }

    fn create_issue(&self, issue: &CanonicalIssue) -> Result<ApiIssue, String> {
        self.post(
            &format!("/repos/{}/issues", self.repository),
            &json!({
                "title": issue.record.title,
                "body": render_issue_body(issue),
            }),
        )
    }

    fn patch_issue(&self, number: u64, payload: &serde_json::Value) -> Result<ApiIssue, String> {
        self.patch(
            &format!("/repos/{}/issues/{number}", self.repository),
            payload,
        )
    }

    fn assert_not_stale(
        &self,
        root: &Path,
        remote_name: &str,
        canonical_id: &str,
        number: u64,
    ) -> Result<(), String> {
        let revision = load_revision(root, remote_name, canonical_id)?.ok_or_else(|| {
            format!(
                "refusing update of {canonical_id}: no observed GitHub revision; run `allodium github observe` and re-plan"
            )
        })?;
        let live = self.fetch_issue(number)?;
        if revision.number != number || revision.remote_updated_at != live.updated_at {
            return Err(format!(
                "refusing stale update of {canonical_id}: GitHub changed after the recorded observation; observe and re-plan before applying"
            ));
        }
        Ok(())
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.send(self.request(self.client.get(api_url(path))))
    }

    fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<T, String> {
        self.send(self.request(self.client.post(api_url(path))).json(payload))
    }

    fn patch<T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<T, String> {
        self.send(self.request(self.client.patch(api_url(path))).json(payload))
    }

    fn request(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, String> {
        let response = request
            .send()
            .map_err(|error| format!("GitHub request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|_| "<unreadable response body>".into());
            return Err(format!("GitHub API returned {status}: {body}"));
        }
        response
            .json()
            .map_err(|error| format!("invalid GitHub API response: {error}"))
    }

    fn require_write_token(&self) -> Result<(), String> {
        if self.token.is_none() {
            return Err(
                "GitHub mutation requires ALLODIUM_GITHUB_TOKEN, GH_TOKEN, or GITHUB_TOKEN".into(),
            );
        }
        Ok(())
    }
}

pub fn load_plan(path: impl AsRef<Path>) -> Result<GitHubPlan, String> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

fn github_token_from_environment() -> Option<String> {
    ["ALLODIUM_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.trim().is_empty()))
}

fn api_url(path: &str) -> String {
    format!("{API_BASE}{path}")
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn load_mappings(root: &Path, remote_name: &str) -> Result<IssueMappings, String> {
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
        return Err(format!("{}: unsupported mapping schema", path.display()));
    }
    Ok(mappings)
}

fn save_mappings(root: &Path, remote_name: &str, mappings: &IssueMappings) -> Result<(), String> {
    let path = root
        .join(".project/remotes")
        .join(remote_name)
        .join("mappings/issues.toml");
    let text = toml::to_string_pretty(mappings)
        .map_err(|error| format!("could not serialize GitHub mappings: {error}"))?;
    fs::create_dir_all(path.parent().expect("mapping path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_observed_issue(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    issue: &ApiIssue,
    observed_at: &str,
) -> Result<(), String> {
    let directory = root
        .join(".project/remotes")
        .join(remote_name)
        .join("observed/issues");
    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;

    let observed = ObservedIssue {
        schema: OBSERVED_ISSUE_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        number: issue.number,
        state: issue.state.clone(),
        title: issue.title.clone(),
        url: issue.html_url.clone(),
        observed_at: observed_at.into(),
    };
    let revision = ObservedRevision {
        schema: OBSERVED_REVISION_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        number: issue.number,
        remote_updated_at: issue.updated_at.clone(),
        observed_at: observed_at.into(),
    };

    write_toml(directory.join(format!("{canonical_id}.toml")), &observed)?;
    fs::write(
        directory.join(format!("{canonical_id}.body.md")),
        issue.body.as_deref().unwrap_or_default(),
    )
    .map_err(|error| format!("could not write observed GitHub issue body: {error}"))?;
    write_toml(
        directory.join(format!("{canonical_id}.revision.toml")),
        &revision,
    )?;
    Ok(())
}

fn load_revision(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
) -> Result<Option<ObservedRevision>, String> {
    let path = root
        .join(".project/remotes")
        .join(remote_name)
        .join("observed/issues")
        .join(format!("{canonical_id}.revision.toml"));
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let revision: ObservedRevision =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if revision.schema != OBSERVED_REVISION_SCHEMA_V0 {
        return Err(format!("{}: unsupported revision schema", path.display()));
    }
    Ok(Some(revision))
}

fn archive_managed_change_if_needed(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    live: &ApiIssue,
    observed_at: &str,
) -> Result<usize, String> {
    let directory = root
        .join(".project/remotes")
        .join(remote_name)
        .join("observed/issues");
    let metadata_path = directory.join(format!("{canonical_id}.toml"));
    let body_path = directory.join(format!("{canonical_id}.body.md"));
    if !metadata_path.exists() || !body_path.exists() {
        return Ok(0);
    }

    let old_text = fs::read_to_string(&metadata_path)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let old: ObservedIssue = toml::from_str(&old_text)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let old_body = fs::read_to_string(&body_path)
        .map_err(|error| format!("{}: {error}", body_path.display()))?;
    let new_body = live.body.as_deref().unwrap_or_default();
    let mut fields = Vec::new();
    if old.title != live.title {
        fields.push("title".into());
    }
    if old.state != live.state {
        fields.push("state".into());
    }
    if old_body != new_body {
        fields.push("body".into());
    }
    if fields.is_empty() {
        return Ok(0);
    }

    let event_id = format!(
        "{remote_name}-issue-{}-managed-change-{}",
        live.number,
        timestamp_slug(&live.updated_at)
    );
    let event_directory = incoming_directory(root, remote_name, observed_at, &event_id)?;
    if event_directory.exists() {
        return Ok(0);
    }
    fs::create_dir_all(&event_directory)
        .map_err(|error| format!("{}: {error}", event_directory.display()))?;
    let event = ManagedChangeEvent {
        schema: MANAGED_CHANGE_SCHEMA_V0.into(),
        id: event_id,
        remote: remote_name.into(),
        kind: "issue.managed_fields.observed_change".into(),
        observed_at: observed_at.into(),
        evidence: "GitHub issue current-state observation changed since the previous local observation; the REST issue representation does not identify the editing actor, so no actor is asserted.".into(),
        fields,
        target: IncomingTarget {
            canonical_id: canonical_id.into(),
            remote_type: "issue".into(),
            remote_id: live.number.to_string(),
        },
        source: ManagedChangeSource {
            remote_object_type: "issue".into(),
            remote_object_id: live.number.to_string(),
            url: live.html_url.clone(),
            remote_updated_at: live.updated_at.clone(),
        },
    };
    write_toml(event_directory.join("event.toml"), &event)?;
    fs::write(event_directory.join("before.toml"), old_text)
        .map_err(|error| format!("could not preserve prior GitHub observation: {error}"))?;
    fs::write(event_directory.join("before.body.md"), old_body)
        .map_err(|error| format!("could not preserve prior GitHub body: {error}"))?;
    let after = ObservedIssue {
        schema: OBSERVED_ISSUE_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        number: live.number,
        state: live.state.clone(),
        title: live.title.clone(),
        url: live.html_url.clone(),
        observed_at: observed_at.into(),
    };
    write_toml(event_directory.join("after.toml"), &after)?;
    fs::write(event_directory.join("after.body.md"), new_body)
        .map_err(|error| format!("could not preserve new GitHub body: {error}"))?;
    Ok(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommentArchive {
    Created,
    Edited,
    Unchanged,
}

fn archive_comment_observation(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    issue_number: u64,
    comment: &ApiComment,
    observed_at: &str,
) -> Result<CommentArchive, String> {
    let directory = root
        .join(".project/remotes")
        .join(remote_name)
        .join("observed/comments");
    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let metadata_path = directory.join(format!("{}.toml", comment.id));
    let body_path = directory.join(format!("{}.body.md", comment.id));
    let new_body = comment.body.as_deref().unwrap_or_default();

    if !metadata_path.exists() {
        let incoming = IncomingIssueComment {
            canonical_id: canonical_id.into(),
            issue_number,
            comment_id: comment.id,
            actor_remote_id: comment.user.id.to_string(),
            actor_login: comment.user.login.clone(),
            url: comment.html_url.clone(),
            body: new_body.into(),
            observed_at: observed_at.into(),
            created_at: Some(comment.created_at.clone()),
            updated_at: Some(comment.updated_at.clone()),
            evidence: "GitHub REST issue-comment observation".into(),
        };
        match allodium_core::github::archive_issue_comment(root, remote_name, &incoming)? {
            ArchiveOutcome::Created(_) | ArchiveOutcome::Existing(_) => {}
        }
        write_observed_comment(
            &metadata_path,
            &body_path,
            canonical_id,
            issue_number,
            comment,
            observed_at,
        )?;
        return Ok(CommentArchive::Created);
    }

    let old_text = fs::read_to_string(&metadata_path)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let old: ObservedComment = toml::from_str(&old_text)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let old_body = fs::read_to_string(&body_path)
        .map_err(|error| format!("{}: {error}", body_path.display()))?;
    if old.remote_updated_at == comment.updated_at && old_body == new_body {
        return Ok(CommentArchive::Unchanged);
    }

    let event_id = format!(
        "{remote_name}-issue-comment-{}-edited-{}",
        comment.id,
        timestamp_slug(&comment.updated_at)
    );
    let event_directory = incoming_directory(root, remote_name, observed_at, &event_id)?;
    if !event_directory.exists() {
        fs::create_dir_all(&event_directory)
            .map_err(|error| format!("{}: {error}", event_directory.display()))?;
        let event = IncomingEvent {
            schema: INCOMING_EVENT_SCHEMA_V0.into(),
            id: event_id,
            remote: remote_name.into(),
            kind: "issue.comment.edited".into(),
            observed_at: observed_at.into(),
            evidence: "GitHub REST issue-comment observation; provider updated_at identifies the remote revision.".into(),
            target: IncomingTarget {
                canonical_id: canonical_id.into(),
                remote_type: "issue".into(),
                remote_id: issue_number.to_string(),
            },
            actor: IncomingActor {
                remote_id: comment.user.id.to_string(),
                login: comment.user.login.clone(),
            },
            source: IncomingSource {
                remote_object_type: "issue_comment".into(),
                remote_object_id: comment.id.to_string(),
                url: comment.html_url.clone(),
                created_at: Some(comment.created_at.clone()),
                updated_at: Some(comment.updated_at.clone()),
            },
        };
        write_toml(event_directory.join("event.toml"), &event)?;
        fs::write(event_directory.join("body.md"), new_body)
            .map_err(|error| format!("could not archive edited GitHub comment: {error}"))?;
    }
    write_observed_comment(
        &metadata_path,
        &body_path,
        canonical_id,
        issue_number,
        comment,
        observed_at,
    )?;
    Ok(CommentArchive::Edited)
}

fn write_observed_comment(
    metadata_path: &Path,
    body_path: &Path,
    canonical_id: &str,
    issue_number: u64,
    comment: &ApiComment,
    observed_at: &str,
) -> Result<(), String> {
    let observed = ObservedComment {
        schema: OBSERVED_COMMENT_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        issue_number,
        comment_id: comment.id,
        remote_updated_at: comment.updated_at.clone(),
        url: comment.html_url.clone(),
        actor_remote_id: comment.user.id.to_string(),
        actor_login: comment.user.login.clone(),
        observed_at: observed_at.into(),
    };
    write_toml(metadata_path, &observed)?;
    fs::write(body_path, comment.body.as_deref().unwrap_or_default())
        .map_err(|error| format!("{}: {error}", body_path.display()))
}

fn update_payload(issue: &CanonicalIssue, fields: &[String]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for field in fields {
        match field.as_str() {
            "title" => {
                object.insert("title".into(), json!(issue.record.title));
            }
            "body" => {
                object.insert("body".into(), json!(render_issue_body(issue)));
            }
            "state" => {
                object.insert("state".into(), json!(issue.record.state));
            }
            _ => {}
        }
    }
    serde_json::Value::Object(object)
}

fn require_number(operation: &allodium_core::github::GitHubOperation) -> Result<u64, String> {
    operation.number.ok_or_else(|| {
        format!(
            "GitHub operation {:?} for {:?} is missing issue number",
            operation.action, operation.canonical_id
        )
    })
}

fn incoming_directory(
    root: &Path,
    remote_name: &str,
    observed_at: &str,
    event_id: &str,
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
        .join(event_id))
}

fn timestamp_slug(timestamp: &str) -> String {
    timestamp
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn write_toml(path: impl AsRef<Path>, value: &impl Serialize) -> Result<(), String> {
    let path = path.as_ref();
    let text = toml::to_string_pretty(value)
        .map_err(|error| format!("could not serialize {}: {error}", path.display()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn observation_writes_exact_body_and_revision() {
        let root = test_root("observe");
        let issue = ApiIssue {
            number: 7,
            html_url: "https://github.com/owner/repo/issues/7".into(),
            title: "Remote title".into(),
            body: Some("Remote body\n".into()),
            state: "open".into(),
            updated_at: "2026-09-16T12:00:00Z".into(),
        };
        write_observed_issue(
            &root,
            "github",
            "issue-0007",
            &issue,
            "2026-09-16T12:01:00Z",
        )
        .unwrap();

        let directory = root.join(".project/remotes/github/observed/issues");
        assert_eq!(
            fs::read_to_string(directory.join("issue-0007.body.md")).unwrap(),
            "Remote body\n"
        );
        let revision = fs::read_to_string(directory.join("issue-0007.revision.toml")).unwrap();
        assert!(revision.contains("remote_updated_at = \"2026-09-16T12:00:00Z\""));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_remote_fields_are_archived_before_observation_is_replaced() {
        let root = test_root("drift");
        let old = ApiIssue {
            number: 7,
            html_url: "https://github.com/owner/repo/issues/7".into(),
            title: "Old".into(),
            body: Some("Old body".into()),
            state: "open".into(),
            updated_at: "2026-09-16T12:00:00Z".into(),
        };
        write_observed_issue(&root, "github", "issue-0007", &old, "2026-09-16T12:01:00Z").unwrap();
        let new = ApiIssue {
            title: "Edited remotely".into(),
            body: Some("New body".into()),
            updated_at: "2026-09-16T12:02:00Z".into(),
            ..old
        };
        assert_eq!(
            archive_managed_change_if_needed(
                &root,
                "github",
                "issue-0007",
                &new,
                "2026-09-16T12:03:00Z",
            )
            .unwrap(),
            1
        );
        let incoming = root.join(".project/remotes/github/incoming/2026/09");
        let event_dir = fs::read_dir(incoming)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(event_dir.join("before.body.md").exists());
        assert!(event_dir.join("after.body.md").exists());

        fs::remove_dir_all(root).unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "allodium-github-adapter-{name}-{}-{nonce}",
            std::process::id()
        ))
    }
}
