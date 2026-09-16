mod review_ingress;
mod wiki;

use allodium_core::github::{
    ArchiveOutcome, GitHubPlan, INCOMING_EVENT_SCHEMA_V0, ISSUE_MAPPINGS_SCHEMA_V0, IncomingActor,
    IncomingEvent, IncomingIssueComment, IncomingSource, IncomingTarget, IssueMapping,
    IssueMappings, OBSERVED_ISSUE_SCHEMA_V0, OBSERVED_REVIEW_SCHEMA_V0, ObservedIssue,
    ObservedReview, PLAN_SCHEMA_V0, REVIEW_MAPPINGS_SCHEMA_V0, ReviewMapping, ReviewMappings,
    render_issue_body, render_review_body,
};
use allodium_core::{CanonicalIssue, CanonicalReview, load_issues, load_remote, load_reviews};
use chrono::Utc;
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const API_BASE: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const OBSERVED_REVISION_SCHEMA_V0: &str = "allodium.github.observed-revision/v0";
const OBSERVED_COMMENT_SCHEMA_V0: &str = "allodium.github.observed-comment/v0";
const COMMENT_ABSENCE_SCHEMA_V0: &str = "allodium.github.comment-absence-observation/v0";
const MANAGED_CHANGE_SCHEMA_V0: &str = "allodium.github.managed-change-observation/v0";
const UNMAPPED_ISSUE_SCHEMA_V0: &str = "allodium.github.unmapped-issue-observation/v0";

#[derive(Debug, Clone)]
pub struct GitHubAdapter {
    client: Client,
    repository: String,
    token: Option<String>,
    api_base: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObserveReport {
    pub issues_observed: usize,
    pub reviews_observed: usize,
    pub wikis_observed: usize,
    pub wiki_remote_changes_archived: usize,
    pub review_conversation_comment_snapshots_archived: usize,
    pub review_submission_snapshots_archived: usize,
    pub review_inline_comment_snapshots_archived: usize,
    pub review_thread_snapshots_archived: usize,
    pub review_social_disappearances_archived: usize,
    pub comments_archived: usize,
    pub comment_edits_archived: usize,
    pub comment_disappearances_archived: usize,
    pub managed_changes_archived: usize,
    pub review_managed_changes_archived: usize,
    pub unmapped_issues_archived: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub issues_created: usize,
    pub issues_updated: usize,
    pub issues_observed: usize,
    pub reviews_created: usize,
    pub reviews_updated: usize,
    pub reviews_observed: usize,
    pub wikis_observed: usize,
    pub wikis_updated: usize,
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
struct CommentAbsenceEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    target: IncomingTarget,
    last_known_actor: IncomingActor,
    source: CommentAbsenceSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CommentAbsenceSource {
    remote_object_type: String,
    remote_object_id: String,
    url: String,
    last_known_updated_at: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UnmappedIssueObservation {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    title: String,
    state: String,
    actor: IncomingActor,
    source: IncomingSource,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiRepositoryIssue {
    number: u64,
    html_url: String,
    title: String,
    body: Option<String>,
    state: String,
    user: ApiUser,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
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
struct ApiPullRef {
    #[serde(rename = "ref")]
    git_ref: String,
    sha: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiPullRequest {
    number: u64,
    html_url: String,
    title: String,
    body: Option<String>,
    state: String,
    merged_at: Option<String>,
    updated_at: String,
    base: ApiPullRef,
    head: ApiPullRef,
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
            api_base: API_BASE.into(),
        })
    }

    pub fn observe(
        &self,
        root: impl AsRef<Path>,
        remote_name: &str,
    ) -> Result<ObserveReport, String> {
        let root = root.as_ref();
        let issue_mappings = load_mappings(root, remote_name)?;
        let review_mappings = load_review_mappings(root, remote_name)?;
        let mapped_numbers = issue_mappings
            .issues
            .values()
            .map(|mapping| mapping.number)
            .collect::<BTreeSet<_>>();
        let mut report = ObserveReport::default();

        for (canonical_id, mapping) in issue_mappings.issues {
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

            let comments = self.fetch_issue_comments(mapping.number)?;
            let current_comment_ids = comments
                .iter()
                .map(|comment| comment.id)
                .collect::<BTreeSet<_>>();
            for comment in comments {
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
            report.comment_disappearances_archived += archive_missing_comment_observations(
                root,
                remote_name,
                &canonical_id,
                mapping.number,
                &current_comment_ids,
                &now(),
            )?;
        }

        for (canonical_id, mapping) in review_mappings.reviews {
            let observed_at = now();
            let review = self.fetch_pull_request(mapping.number)?;
            report.review_managed_changes_archived += archive_review_managed_change_if_needed(
                root,
                remote_name,
                &canonical_id,
                &review,
                &observed_at,
            )?;
            write_observed_review(root, remote_name, &canonical_id, &review, &observed_at)?;
            report.reviews_observed += 1;
            let social = review_ingress::archive_review_social(
                self,
                root,
                remote_name,
                &canonical_id,
                mapping.number,
                &observed_at,
            )?;
            report.review_conversation_comment_snapshots_archived +=
                social.conversation_comment_snapshots_archived;
            report.review_submission_snapshots_archived +=
                social.review_submission_snapshots_archived;
            report.review_inline_comment_snapshots_archived +=
                social.inline_comment_snapshots_archived;
            report.review_thread_snapshots_archived += social.thread_snapshots_archived;
            report.review_social_disappearances_archived += social.disappearances_archived;
        }

        for issue in self.fetch_repository_issues()? {
            if issue.pull_request.is_some() || mapped_numbers.contains(&issue.number) {
                continue;
            }
            if archive_unmapped_issue_observation(root, remote_name, &issue, &now())? {
                report.unmapped_issues_archived += 1;
            }
        }

        let wiki_report = wiki::observe_wiki(self, root, remote_name)?;
        report.wikis_observed += wiki_report.observed;
        report.wiki_remote_changes_archived += wiki_report.remote_changes_archived;

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
        let reviews = load_reviews(root)?
            .into_iter()
            .map(|review| (review.record.id.clone(), review))
            .collect::<BTreeMap<_, _>>();
        let mut issue_mappings = load_mappings(root, &plan.remote)?;
        let mut review_mappings = load_review_mappings(root, &plan.remote)?;
        let mut report = ApplyReport::default();

        for operation in &plan.operations {
            match operation.action.as_str() {
                "create_issue" => {
                    let issue = require_issue(&issues, &operation.canonical_id)?;
                    self.require_write_token()?;
                    if issue_mappings.issues.contains_key(&operation.canonical_id) {
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
                    issue_mappings.issues.insert(
                        operation.canonical_id.clone(),
                        IssueMapping {
                            number: created.number,
                            url: created.html_url.clone(),
                        },
                    );
                    save_mappings(root, &plan.remote, &issue_mappings)?;
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
                    let _issue = require_issue(&issues, &operation.canonical_id)?;
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
                    let issue = require_issue(&issues, &operation.canonical_id)?;
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
                "create_pull_request" => {
                    let review = require_review(&reviews, &operation.canonical_id)?;
                    self.require_write_token()?;
                    if review_mappings
                        .reviews
                        .contains_key(&operation.canonical_id)
                    {
                        return Err(format!(
                            "refusing create: {:?} already has a GitHub review mapping",
                            operation.canonical_id
                        ));
                    }
                    let created = self.create_pull_request(review)?;
                    review_mappings.reviews.insert(
                        operation.canonical_id.clone(),
                        ReviewMapping {
                            number: created.number,
                            url: created.html_url.clone(),
                        },
                    );
                    save_review_mappings(root, &plan.remote, &review_mappings)?;
                    write_observed_review(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        &created,
                        &now(),
                    )?;
                    report.reviews_created += 1;
                }
                "observe_pull_request" => {
                    let _review = require_review(&reviews, &operation.canonical_id)?;
                    let number = require_number(operation)?;
                    let live = self.fetch_pull_request(number)?;
                    archive_review_managed_change_if_needed(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        &live,
                        &now(),
                    )?;
                    write_observed_review(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        &live,
                        &now(),
                    )?;
                    report.reviews_observed += 1;
                }
                "update_pull_request" => {
                    let review = require_review(&reviews, &operation.canonical_id)?;
                    self.require_write_token()?;
                    let number = require_number(operation)?;
                    self.assert_review_not_stale(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        number,
                    )?;
                    let payload = update_review_payload(review, &operation.fields)?;
                    let updated = self.patch_pull_request(number, &payload)?;
                    write_observed_review(
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        &updated,
                        &now(),
                    )?;
                    report.reviews_updated += 1;
                }
                "observe_wiki" => {
                    let wiki_report = wiki::observe_wiki(self, root, &plan.remote)?;
                    report.wikis_observed += wiki_report.observed;
                }
                "update_wiki" => {
                    wiki::apply_wiki_update(self, root, &plan.remote)?;
                    report.wikis_updated += 1;
                }
                other => return Err(format!("unsupported GitHub plan operation {other:?}")),
            }
        }

        Ok(report)
    }

    fn fetch_issue(&self, number: u64) -> Result<ApiIssue, String> {
        self.get(&format!("/repos/{}/issues/{number}", self.repository))
    }

    fn fetch_pull_request(&self, number: u64) -> Result<ApiPullRequest, String> {
        self.get(&format!("/repos/{}/pulls/{number}", self.repository))
    }

    fn create_pull_request(&self, review: &CanonicalReview) -> Result<ApiPullRequest, String> {
        self.post(
            &format!("/repos/{}/pulls", self.repository),
            &json!({
                "title": review.record.title,
                "body": render_review_body(review),
                "base": review.record.base,
                "head": review.record.head,
            }),
        )
    }

    fn patch_pull_request(
        &self,
        number: u64,
        payload: &serde_json::Value,
    ) -> Result<ApiPullRequest, String> {
        self.patch(
            &format!("/repos/{}/pulls/{number}", self.repository),
            payload,
        )
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

    fn fetch_repository_issues(&self) -> Result<Vec<ApiRepositoryIssue>, String> {
        let mut page = 1;
        let mut issues = Vec::new();
        loop {
            let batch: Vec<ApiRepositoryIssue> = self.get(&format!(
                "/repos/{}/issues?state=all&per_page=100&page={page}",
                self.repository
            ))?;
            let count = batch.len();
            issues.extend(batch);
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(issues)
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

    fn assert_review_not_stale(
        &self,
        root: &Path,
        remote_name: &str,
        canonical_id: &str,
        number: u64,
    ) -> Result<(), String> {
        let observed = load_observed_review(root, remote_name, canonical_id)?.ok_or_else(|| {
            format!(
                "refusing update of {canonical_id}: no observed GitHub pull-request revision; run `allodium github observe` and re-plan"
            )
        })?;
        let live = self.fetch_pull_request(number)?;
        if observed.number != number
            || observed.remote_updated_at != live.updated_at
            || observed.base_sha != live.base.sha
            || observed.head_sha != live.head.sha
        {
            return Err(format!(
                "refusing stale update of {canonical_id}: GitHub pull request changed after the recorded observation; observe and re-plan before applying"
            ));
        }
        Ok(())
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.send(self.request(self.client.get(self.api_url(path))))
    }

    fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<T, String> {
        self.send(
            self.request(self.client.post(self.api_url(path)))
                .json(payload),
        )
    }

    fn patch<T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<T, String> {
        self.send(
            self.request(self.client.patch(self.api_url(path)))
                .json(payload),
        )
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.api_base.trim_end_matches('/'), path)
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
            return Err(github_api_error(status, &body));
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

fn github_api_error(status: reqwest::StatusCode, body: &str) -> String {
    if status == reqwest::StatusCode::FORBIDDEN
        && body.contains("GitHub Actions is not permitted to create or approve pull requests")
    {
        return format!(
            "GitHub capability blocked: the repository-level Actions policy forbids GITHUB_TOKEN from creating or approving pull requests even when the workflow has pull-requests: write. The canonical review was not mapped or mutated. Enable GitHub's repository setting that allows Actions to create/approve pull requests, or execute the same explicit Allodium plan with a different authorized token/executor. Provider response: {body}"
        );
    }
    format!("GitHub API returned {status}: {body}")
}

fn github_token_from_environment() -> Option<String> {
    ["ALLODIUM_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.trim().is_empty()))
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

fn load_review_mappings(root: &Path, remote_name: &str) -> Result<ReviewMappings, String> {
    let path = root
        .join(".project/remotes")
        .join(remote_name)
        .join("mappings/reviews.toml");
    if !path.exists() {
        return Ok(ReviewMappings {
            schema: REVIEW_MAPPINGS_SCHEMA_V0.into(),
            reviews: BTreeMap::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: ReviewMappings =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != REVIEW_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported review mapping schema",
            path.display()
        ));
    }
    Ok(mappings)
}

fn save_review_mappings(
    root: &Path,
    remote_name: &str,
    mappings: &ReviewMappings,
) -> Result<(), String> {
    let path = root
        .join(".project/remotes")
        .join(remote_name)
        .join("mappings/reviews.toml");
    let text = toml::to_string_pretty(mappings)
        .map_err(|error| format!("could not serialize GitHub review mappings: {error}"))?;
    fs::create_dir_all(path.parent().expect("mapping path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

fn normalized_review_state(review: &ApiPullRequest) -> String {
    if review.merged_at.is_some() {
        "merged".into()
    } else {
        review.state.clone()
    }
}

fn write_observed_review(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    review: &ApiPullRequest,
    observed_at: &str,
) -> Result<(), String> {
    let directory = root
        .join(".project/remotes")
        .join(remote_name)
        .join("observed/reviews");
    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let observed = ObservedReview {
        schema: OBSERVED_REVIEW_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        number: review.number,
        state: normalized_review_state(review),
        title: review.title.clone(),
        url: review.html_url.clone(),
        base_ref: review.base.git_ref.clone(),
        head_ref: review.head.git_ref.clone(),
        base_sha: review.base.sha.clone(),
        head_sha: review.head.sha.clone(),
        merged_at: review.merged_at.clone(),
        remote_updated_at: review.updated_at.clone(),
        observed_at: observed_at.into(),
    };
    write_toml(directory.join(format!("{canonical_id}.toml")), &observed)?;
    fs::write(
        directory.join(format!("{canonical_id}.body.md")),
        review.body.as_deref().unwrap_or_default(),
    )
    .map_err(|error| format!("could not write observed GitHub pull-request body: {error}"))
}

fn load_observed_review(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
) -> Result<Option<ObservedReview>, String> {
    let path = root
        .join(".project/remotes")
        .join(remote_name)
        .join("observed/reviews")
        .join(format!("{canonical_id}.toml"));
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedReview =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_REVIEW_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported observed review schema",
            path.display()
        ));
    }
    Ok(Some(observed))
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

fn archive_unmapped_issue_observation(
    root: &Path,
    remote_name: &str,
    issue: &ApiRepositoryIssue,
    observed_at: &str,
) -> Result<bool, String> {
    let event_id = format!(
        "{remote_name}-issue-{}-unmapped-{}",
        issue.number,
        timestamp_slug(&issue.updated_at)
    );
    let directory = incoming_directory(root, remote_name, &issue.updated_at, &event_id)?;
    let event = UnmappedIssueObservation {
        schema: UNMAPPED_ISSUE_SCHEMA_V0.into(),
        id: event_id,
        remote: remote_name.into(),
        kind: "issue.unmapped.observed".into(),
        observed_at: observed_at.into(),
        evidence: "GitHub REST issue observation has no canonical Allodium mapping; archived as provider-scoped evidence only. No canonical issue or mapping was created.".into(),
        title: issue.title.clone(),
        state: issue.state.clone(),
        actor: IncomingActor {
            remote_id: issue.user.id.to_string(),
            login: issue.user.login.clone(),
        },
        source: IncomingSource {
            remote_object_type: "issue".into(),
            remote_object_id: issue.number.to_string(),
            url: issue.html_url.clone(),
            created_at: Some(issue.created_at.clone()),
            updated_at: Some(issue.updated_at.clone()),
        },
    };
    let event_path = directory.join("event.toml");
    let body_path = directory.join("body.md");
    let event_text = toml::to_string_pretty(&event)
        .map_err(|error| format!("could not serialize unmapped GitHub issue: {error}"))?;
    let body = issue.body.as_deref().unwrap_or_default();

    if directory.exists() {
        let existing_event_text = fs::read_to_string(&event_path)
            .map_err(|error| format!("{}: {error}", event_path.display()))?;
        let existing_event: UnmappedIssueObservation = toml::from_str(&existing_event_text)
            .map_err(|error| format!("{}: {error}", event_path.display()))?;
        let existing_body = fs::read_to_string(&body_path)
            .map_err(|error| format!("{}: {error}", body_path.display()))?;
        let same_revision = existing_event.schema == event.schema
            && existing_event.id == event.id
            && existing_event.remote == event.remote
            && existing_event.kind == event.kind
            && existing_event.title == event.title
            && existing_event.state == event.state
            && existing_event.actor == event.actor
            && existing_event.source == event.source;
        if same_revision && existing_body == body {
            return Ok(false);
        }
        return Err(format!(
            "unmapped GitHub issue observation collision at {}; provider revision identity points to different evidence",
            directory.display()
        ));
    }

    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    fs::write(&event_path, event_text)
        .map_err(|error| format!("{}: {error}", event_path.display()))?;
    fs::write(&body_path, body).map_err(|error| format!("{}: {error}", body_path.display()))?;
    Ok(true)
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
    if old_body.trim_end() != new_body.trim_end() {
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

fn archive_review_managed_change_if_needed(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    live: &ApiPullRequest,
    observed_at: &str,
) -> Result<usize, String> {
    let directory = root
        .join(".project/remotes")
        .join(remote_name)
        .join("observed/reviews");
    let metadata_path = directory.join(format!("{canonical_id}.toml"));
    let body_path = directory.join(format!("{canonical_id}.body.md"));
    if !metadata_path.exists() || !body_path.exists() {
        return Ok(0);
    }
    let old_text = fs::read_to_string(&metadata_path)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let old: ObservedReview = toml::from_str(&old_text)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let old_body = fs::read_to_string(&body_path)
        .map_err(|error| format!("{}: {error}", body_path.display()))?;
    let new_body = live.body.as_deref().unwrap_or_default();
    let live_state = normalized_review_state(live);
    let mut fields = Vec::new();
    if old.title != live.title {
        fields.push("title".into());
    }
    if old.state != live_state {
        fields.push("state".into());
    }
    if old.base_ref != live.base.git_ref {
        fields.push("base".into());
    }
    if old.head_ref != live.head.git_ref {
        fields.push("head".into());
    }
    if old_body.trim_end() != new_body.trim_end() {
        fields.push("body".into());
    }
    if fields.is_empty() {
        return Ok(0);
    }
    let event_id = format!(
        "{remote_name}-pull-request-{}-managed-change-{}",
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
        kind: "review.managed_fields.observed_change".into(),
        observed_at: observed_at.into(),
        evidence: "GitHub pull-request current-state observation changed since the previous local observation; the REST pull-request representation does not identify the editing actor, so no actor is asserted.".into(),
        fields,
        target: IncomingTarget {
            canonical_id: canonical_id.into(),
            remote_type: "pull_request".into(),
            remote_id: live.number.to_string(),
        },
        source: ManagedChangeSource {
            remote_object_type: "pull_request".into(),
            remote_object_id: live.number.to_string(),
            url: live.html_url.clone(),
            remote_updated_at: live.updated_at.clone(),
        },
    };
    write_toml(event_directory.join("event.toml"), &event)?;
    fs::write(event_directory.join("before.toml"), old_text)
        .map_err(|error| format!("could not preserve prior GitHub review observation: {error}"))?;
    fs::write(event_directory.join("before.body.md"), old_body)
        .map_err(|error| format!("could not preserve prior GitHub review body: {error}"))?;
    let after = ObservedReview {
        schema: OBSERVED_REVIEW_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        number: live.number,
        state: live_state,
        title: live.title.clone(),
        url: live.html_url.clone(),
        base_ref: live.base.git_ref.clone(),
        head_ref: live.head.git_ref.clone(),
        base_sha: live.base.sha.clone(),
        head_sha: live.head.sha.clone(),
        merged_at: live.merged_at.clone(),
        remote_updated_at: live.updated_at.clone(),
        observed_at: observed_at.into(),
    };
    write_toml(event_directory.join("after.toml"), &after)?;
    fs::write(event_directory.join("after.body.md"), new_body)
        .map_err(|error| format!("could not preserve new GitHub review body: {error}"))?;
    Ok(1)
}

fn archive_missing_comment_observations(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    issue_number: u64,
    current_comment_ids: &BTreeSet<u64>,
    observed_at: &str,
) -> Result<usize, String> {
    let directory = root
        .join(".project/remotes")
        .join(remote_name)
        .join("observed/comments");
    if !directory.exists() {
        return Ok(0);
    }

    let mut archived = 0;
    for entry in
        fs::read_dir(&directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let text =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let previous: ObservedComment =
            toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        if previous.issue_number != issue_number
            || current_comment_ids.contains(&previous.comment_id)
        {
            continue;
        }
        let body_path = directory.join(format!("{}.body.md", previous.comment_id));
        let body = fs::read_to_string(&body_path)
            .map_err(|error| format!("{}: {error}", body_path.display()))?;
        if archive_comment_absence(
            root,
            remote_name,
            canonical_id,
            &previous,
            &body,
            observed_at,
        )? {
            archived += 1;
        }
    }
    Ok(archived)
}

fn archive_comment_absence(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    previous: &ObservedComment,
    body: &str,
    observed_at: &str,
) -> Result<bool, String> {
    let event_id = format!(
        "{remote_name}-issue-comment-{}-no-longer-observed-after-{}",
        previous.comment_id,
        timestamp_slug(&previous.remote_updated_at)
    );
    // GitHub supplies no deletion timestamp in a current comment listing. Bucket
    // by the last known provider revision so repeated absence polls resolve to one event.
    let event_directory =
        incoming_directory(root, remote_name, &previous.remote_updated_at, &event_id)?;
    let event = CommentAbsenceEvent {
        schema: COMMENT_ABSENCE_SCHEMA_V0.into(),
        id: event_id,
        remote: remote_name.into(),
        kind: "issue.comment.no_longer_observed".into(),
        observed_at: observed_at.into(),
        evidence: "A previously observed GitHub issue comment is absent from the current REST comment listing. This proves only that the comment is no longer observed through this surface; no deletion actor or exact deletion time is asserted.".into(),
        target: IncomingTarget {
            canonical_id: canonical_id.into(),
            remote_type: "issue".into(),
            remote_id: previous.issue_number.to_string(),
        },
        last_known_actor: IncomingActor {
            remote_id: previous.actor_remote_id.clone(),
            login: previous.actor_login.clone(),
        },
        source: CommentAbsenceSource {
            remote_object_type: "issue_comment".into(),
            remote_object_id: previous.comment_id.to_string(),
            url: previous.url.clone(),
            last_known_updated_at: previous.remote_updated_at.clone(),
        },
    };
    let event_path = event_directory.join("event.toml");
    let body_path = event_directory.join("last-known.body.md");
    if event_directory.exists() {
        let existing_text = fs::read_to_string(&event_path)
            .map_err(|error| format!("{}: {error}", event_path.display()))?;
        let existing: CommentAbsenceEvent = toml::from_str(&existing_text)
            .map_err(|error| format!("{}: {error}", event_path.display()))?;
        let existing_body = fs::read_to_string(&body_path)
            .map_err(|error| format!("{}: {error}", body_path.display()))?;
        let same_evidence = existing.schema == event.schema
            && existing.id == event.id
            && existing.remote == event.remote
            && existing.kind == event.kind
            && existing.evidence == event.evidence
            && existing.target == event.target
            && existing.last_known_actor == event.last_known_actor
            && existing.source == event.source;
        if same_evidence && existing_body == body {
            return Ok(false);
        }
        return Err(format!(
            "comment absence observation collision at {}; stable last-known revision points to different evidence",
            event_directory.display()
        ));
    }

    fs::create_dir_all(&event_directory)
        .map_err(|error| format!("{}: {error}", event_directory.display()))?;
    write_toml(&event_path, &event)?;
    fs::write(&body_path, body).map_err(|error| format!("{}: {error}", body_path.display()))?;
    Ok(true)
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

fn update_review_payload(
    review: &CanonicalReview,
    fields: &[String],
) -> Result<serde_json::Value, String> {
    let mut object = serde_json::Map::new();
    for field in fields {
        match field.as_str() {
            "title" => {
                object.insert("title".into(), json!(review.record.title));
            }
            "body" => {
                object.insert("body".into(), json!(render_review_body(review)));
            }
            "base" => {
                object.insert("base".into(), json!(review.record.base));
            }
            "state" => {
                if review.record.state == "merged" {
                    return Err("merged review state is observation-only; it is not a pull-request update payload".into());
                }
                object.insert("state".into(), json!(review.record.state));
            }
            "head" => {
                return Err("GitHub cannot retarget the head of an existing pull request".into());
            }
            _ => {}
        }
    }
    Ok(serde_json::Value::Object(object))
}

fn require_issue<'a>(
    issues: &'a BTreeMap<String, CanonicalIssue>,
    canonical_id: &str,
) -> Result<&'a CanonicalIssue, String> {
    issues
        .get(canonical_id)
        .ok_or_else(|| format!("plan references missing canonical issue {canonical_id:?}"))
}

fn require_review<'a>(
    reviews: &'a BTreeMap<String, CanonicalReview>,
    canonical_id: &str,
) -> Result<&'a CanonicalReview, String> {
    reviews
        .get(canonical_id)
        .ok_or_else(|| format!("plan references missing canonical review {canonical_id:?}"))
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
mod api_error_tests {
    use super::*;

    #[test]
    fn actions_pull_request_policy_block_is_diagnosed_as_capability_boundary() {
        let error = github_api_error(
            reqwest::StatusCode::FORBIDDEN,
            r#"{"message":"GitHub Actions is not permitted to create or approve pull requests."}"#,
        );
        assert!(error.contains("GitHub capability blocked"));
        assert!(error.contains("repository-level Actions policy"));
        assert!(error.contains("canonical review was not mapped or mutated"));
    }
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

    #[test]
    fn unmapped_issue_revision_is_provider_scoped_and_idempotent() {
        let root = test_root("unmapped");
        let issue = ApiRepositoryIssue {
            number: 44,
            html_url: "https://github.com/owner/repo/issues/44".into(),
            title: "External issue".into(),
            body: Some("Remote-only body".into()),
            state: "open".into(),
            user: ApiUser {
                id: 9,
                login: "outside-user".into(),
            },
            created_at: "2026-09-16T10:00:00Z".into(),
            updated_at: "2026-09-16T10:05:00Z".into(),
            pull_request: None,
        };

        assert!(
            archive_unmapped_issue_observation(&root, "github", &issue, "2026-09-16T11:00:00Z",)
                .unwrap()
        );
        assert!(
            !archive_unmapped_issue_observation(&root, "github", &issue, "2026-09-17T11:00:00Z",)
                .unwrap()
        );
        assert!(!root.join(".project/issues").exists());
        let event = root.join(
            ".project/remotes/github/incoming/2026/09/github-issue-44-unmapped-2026-09-16T10-05-00Z/event.toml",
        );
        let text = fs::read_to_string(event).unwrap();
        assert!(text.contains("kind = \"issue.unmapped.observed\""));
        assert!(text.contains("remote_object_id = \"44\""));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn review_observation_writes_refs_revisions_and_body() {
        let root = test_root("review-observe");
        let review = ApiPullRequest {
            number: 23,
            html_url: "https://github.com/owner/repo/pull/23".into(),
            title: "Remote review".into(),
            body: Some("Remote review body\n".into()),
            state: "open".into(),
            merged_at: None,
            updated_at: "2026-09-16T12:00:00Z".into(),
            base: ApiPullRef {
                git_ref: "main".into(),
                sha: "base-sha".into(),
            },
            head: ApiPullRef {
                git_ref: "feature".into(),
                sha: "head-sha".into(),
            },
        };
        write_observed_review(
            &root,
            "github",
            "review-0001",
            &review,
            "2026-09-16T12:01:00Z",
        )
        .unwrap();
        let directory = root.join(".project/remotes/github/observed/reviews");
        let metadata = fs::read_to_string(directory.join("review-0001.toml")).unwrap();
        assert!(metadata.contains("base_ref = \"main\""));
        assert!(metadata.contains("head_sha = \"head-sha\""));
        assert!(metadata.contains("remote_updated_at = \"2026-09-16T12:00:00Z\""));
        assert_eq!(
            fs::read_to_string(directory.join("review-0001.body.md")).unwrap(),
            "Remote review body\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_review_update_is_rejected_before_patch_is_sent() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        let root = test_root("stale-review-http");
        fs::create_dir_all(root.join(".project/reviews/review-0001")).unwrap();
        fs::write(
            root.join(".project/reviews/review-0001/review.toml"),
            "schema = \"allodium.review/v0\"\nid = \"review-0001\"\ntitle = \"Canonical review\"\nstate = \"open\"\nbase = \"main\"\nhead = \"feature\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/reviews/review-0001/body.md"),
            "Canonical body",
        )
        .unwrap();
        fs::create_dir_all(root.join(".project/remotes/github/observed/reviews")).unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/remotes/github/observed/reviews/review-0001.toml"),
            "schema = \"allodium.github.observed-review/v0\"\ncanonical_id = \"review-0001\"\nnumber = 23\nstate = \"open\"\ntitle = \"Old title\"\nurl = \"https://github.com/owner/repo/pull/23\"\nbase_ref = \"main\"\nhead_ref = \"feature\"\nbase_sha = \"base-old\"\nhead_sha = \"head-old\"\nremote_updated_at = \"2026-09-16T10:00:00Z\"\nobserved_at = \"2026-09-16T10:01:00Z\"\n",
        )
        .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let server_requests = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let server = thread::spawn(move || {
            while !server_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut buffer = [0_u8; 8192];
                        let size = stream.read(&mut buffer).unwrap();
                        let request = String::from_utf8_lossy(&buffer[..size]);
                        server_requests
                            .lock()
                            .unwrap()
                            .push(request.lines().next().unwrap_or_default().to_string());
                        let body = r#"{"number":23,"html_url":"https://github.com/owner/repo/pull/23","title":"Changed remotely","body":"Remote body","state":"open","merged_at":null,"updated_at":"2026-09-16T10:02:00Z","base":{"ref":"main","sha":"base-new"},"head":{"ref":"feature","sha":"head-new"}}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        stream.write_all(response.as_bytes()).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("fake GitHub server failed: {error}"),
                }
            }
        });
        let adapter = GitHubAdapter {
            client: Client::builder().build().unwrap(),
            repository: "owner/repo".into(),
            token: Some("test-token".into()),
            api_base: format!("http://{address}"),
        };
        let plan = GitHubPlan {
            schema: PLAN_SCHEMA_V0.into(),
            remote: "github".into(),
            repository: "owner/repo".into(),
            operations: vec![allodium_core::github::GitHubOperation {
                canonical_id: "review-0001".into(),
                action: "update_pull_request".into(),
                number: Some(23),
                fields: vec!["title".into()],
                reason: "HTTP stale-review regression".into(),
            }],
        };
        let error = adapter.apply(&root, &plan).unwrap_err();
        assert!(error.contains("refusing stale update of review-0001"));
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            1,
            "stale review apply emitted extra HTTP request: {requests:?}"
        );
        assert!(requests[0].starts_with("GET /repos/owner/repo/pulls/23 "));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_update_is_rejected_before_patch_is_sent() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        let root = test_root("stale-http");
        fs::create_dir_all(root.join(".project/issues/issue-0007")).unwrap();
        fs::write(
            root.join(".project/issues/issue-0007/issue.toml"),
            "schema = \"allodium.issue/v0\"\nid = \"issue-0007\"\ntitle = \"Canonical title\"\nstate = \"open\"\nlabels = []\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/issues/issue-0007/body.md"),
            "Canonical body",
        )
        .unwrap();
        fs::create_dir_all(root.join(".project/remotes/github/observed/issues")).unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/remotes/github/observed/issues/issue-0007.revision.toml"),
            "schema = \"allodium.github.observed-revision/v0\"\ncanonical_id = \"issue-0007\"\nnumber = 7\nremote_updated_at = \"2026-09-16T10:00:00Z\"\nobserved_at = \"2026-09-16T10:01:00Z\"\n",
        )
        .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let server_requests = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let server = thread::spawn(move || {
            while !server_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut buffer = [0_u8; 8192];
                        let size = stream.read(&mut buffer).unwrap();
                        let request = String::from_utf8_lossy(&buffer[..size]);
                        let request_line = request.lines().next().unwrap_or_default().to_string();
                        server_requests.lock().unwrap().push(request_line);
                        let body = r#"{"number":7,"html_url":"https://github.com/owner/repo/issues/7","title":"Changed remotely","body":"Remote body","state":"open","updated_at":"2026-09-16T10:02:00Z"}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        stream.write_all(response.as_bytes()).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("fake GitHub server failed: {error}"),
                }
            }
        });

        let adapter = GitHubAdapter {
            client: Client::builder().build().unwrap(),
            repository: "owner/repo".into(),
            token: Some("test-token".into()),
            api_base: format!("http://{address}"),
        };
        let plan = GitHubPlan {
            schema: PLAN_SCHEMA_V0.into(),
            remote: "github".into(),
            repository: "owner/repo".into(),
            operations: vec![allodium_core::github::GitHubOperation {
                canonical_id: "issue-0007".into(),
                action: "update_issue".into(),
                number: Some(7),
                fields: vec!["title".into()],
                reason: "HTTP stale-write regression".into(),
            }],
        };

        let error = adapter.apply(&root, &plan).unwrap_err();
        assert!(error.contains("refusing stale update of issue-0007"));
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            1,
            "stale apply emitted an unexpected second HTTP request: {requests:?}"
        );
        assert!(requests[0].starts_with("GET /repos/owner/repo/issues/7 "));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_comment_is_archived_once_without_claiming_deletion() {
        let root = test_root("comment-absence");
        let directory = root.join(".project/remotes/github/observed/comments");
        fs::create_dir_all(&directory).unwrap();
        let previous = ObservedComment {
            schema: OBSERVED_COMMENT_SCHEMA_V0.into(),
            canonical_id: "issue-0007".into(),
            issue_number: 7,
            comment_id: 77,
            remote_updated_at: "2026-09-16T10:05:00Z".into(),
            url: "https://github.com/owner/repo/issues/7#issuecomment-77".into(),
            actor_remote_id: "9".into(),
            actor_login: "outside-user".into(),
            observed_at: "2026-09-16T10:06:00Z".into(),
        };
        write_toml(directory.join("77.toml"), &previous).unwrap();
        fs::write(directory.join("77.body.md"), "last known body").unwrap();
        let current = BTreeSet::new();

        assert_eq!(
            archive_missing_comment_observations(
                &root,
                "github",
                "issue-0007",
                7,
                &current,
                "2026-09-16T11:00:00Z",
            )
            .unwrap(),
            1
        );
        assert_eq!(
            archive_missing_comment_observations(
                &root,
                "github",
                "issue-0007",
                7,
                &current,
                "2026-09-17T11:00:00Z",
            )
            .unwrap(),
            0
        );
        let event = root.join(
            ".project/remotes/github/incoming/2026/09/github-issue-comment-77-no-longer-observed-after-2026-09-16T10-05-00Z/event.toml",
        );
        let text = fs::read_to_string(event).unwrap();
        assert!(text.contains("kind = \"issue.comment.no_longer_observed\""));
        assert!(text.contains("no deletion actor or exact deletion time is asserted"));
        assert!(directory.join("77.toml").exists());
        assert!(directory.join("77.body.md").exists());

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
