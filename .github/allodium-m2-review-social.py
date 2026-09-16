from pathlib import Path

root = Path('.')
lib = root / 'crates/allodium-github/src/lib.rs'
cli = root / 'crates/allodium-cli/src/main.rs'
workflow = root / '.github/workflows/allodium-sync.yml'
module = root / 'crates/allodium-github/src/review_ingress.rs'

module_source = r'''use super::{GitHubAdapter, incoming_directory, write_toml};
use allodium_core::github::{IncomingActor, IncomingSource, IncomingTarget};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const REVIEW_SOCIAL_EVENT_SCHEMA_V0: &str = "allodium.github.review-social-event/v0";
const OBSERVED_REVIEW_SOCIAL_SCHEMA_V0: &str = "allodium.github.observed-review-social/v0";
const REVIEW_SOCIAL_ABSENCE_SCHEMA_V0: &str = "allodium.github.review-social-absence/v0";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ReviewIngressReport {
    pub conversation_comment_snapshots_archived: usize,
    pub review_submission_snapshots_archived: usize,
    pub inline_comment_snapshots_archived: usize,
    pub thread_snapshots_archived: usize,
    pub disappearances_archived: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiActor {
    id: u64,
    login: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiConversationComment {
    id: u64,
    html_url: String,
    body: Option<String>,
    user: ApiActor,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiReviewSubmission {
    id: u64,
    html_url: String,
    body: Option<String>,
    user: ApiActor,
    state: String,
    submitted_at: Option<String>,
    commit_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiInlineComment {
    id: u64,
    html_url: String,
    body: Option<String>,
    user: ApiActor,
    created_at: String,
    updated_at: String,
    pull_request_review_id: Option<u64>,
    in_reply_to_id: Option<u64>,
    path: String,
    commit_id: String,
    original_commit_id: String,
    line: Option<i64>,
    side: Option<String>,
    start_line: Option<i64>,
    start_side: Option<String>,
    original_line: Option<i64>,
    original_start_line: Option<i64>,
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
    #[serde(rename = "pullRequest")]
    pull_request: Option<GraphPullRequest>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPullRequest {
    #[serde(rename = "reviewThreads")]
    review_threads: GraphThreadConnection,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphThreadConnection {
    nodes: Vec<GraphThread>,
    #[serde(rename = "pageInfo")]
    page_info: GraphPageInfo,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphThread {
    id: String,
    #[serde(rename = "isResolved")]
    is_resolved: bool,
    #[serde(rename = "isOutdated")]
    is_outdated: bool,
    path: String,
    line: Option<i64>,
    #[serde(rename = "startLine")]
    start_line: Option<i64>,
    #[serde(rename = "diffSide")]
    diff_side: Option<String>,
    #[serde(rename = "originalLine")]
    original_line: Option<i64>,
    #[serde(rename = "originalStartLine")]
    original_start_line: Option<i64>,
    comments: GraphThreadComments,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphThreadComments {
    nodes: Vec<GraphThreadComment>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphThreadComment {
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct ReviewSocialContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    review_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pull_request_review_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    in_reply_to_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_commit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_start_line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_resolved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_outdated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor_comment_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReviewSocialEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    target: IncomingTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<IncomingActor>,
    source: IncomingSource,
    context: ReviewSocialContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ObservedReviewSocial {
    schema: String,
    canonical_id: String,
    pull_request_number: u64,
    kind_key: String,
    remote_object_id: String,
    fingerprint: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<IncomingActor>,
    present: bool,
    observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReviewSocialAbsenceEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    target: IncomingTarget,
    source_kind: String,
    source_remote_object_id: String,
    source_url: String,
    last_known_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_known_actor: Option<IncomingActor>,
}

struct SnapshotInput {
    kind_key: &'static str,
    remote_object_type: &'static str,
    remote_object_id: String,
    url: String,
    actor: Option<IncomingActor>,
    body: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    evidence: &'static str,
    context: ReviewSocialContext,
}

pub(super) fn archive_review_social(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    pull_request_number: u64,
    observed_at: &str,
) -> Result<ReviewIngressReport, String> {
    let mut report = ReviewIngressReport::default();

    let conversation = fetch_conversation_comments(adapter, pull_request_number)?;
    let conversation_ids = conversation
        .iter()
        .map(|item| item.id.to_string())
        .collect::<BTreeSet<_>>();
    for item in conversation {
        let snapshot = SnapshotInput {
            kind_key: "conversation-comment",
            remote_object_type: "pull_request_conversation_comment",
            remote_object_id: item.id.to_string(),
            url: item.html_url,
            actor: Some(actor(&item.user)),
            body: item.body,
            created_at: Some(item.created_at),
            updated_at: Some(item.updated_at),
            evidence: "GitHub REST pull-request conversation comment observation",
            context: ReviewSocialContext::default(),
        };
        report.conversation_comment_snapshots_archived += usize::from(archive_snapshot(
            root,
            remote_name,
            canonical_id,
            pull_request_number,
            observed_at,
            snapshot,
        )?);
    }
    report.disappearances_archived += archive_missing(
        root,
        remote_name,
        canonical_id,
        pull_request_number,
        observed_at,
        "conversation-comment",
        &conversation_ids,
    )?;

    let submissions = fetch_review_submissions(adapter, pull_request_number)?;
    let submission_ids = submissions
        .iter()
        .map(|item| item.id.to_string())
        .collect::<BTreeSet<_>>();
    for item in submissions {
        let snapshot = SnapshotInput {
            kind_key: "review-submission",
            remote_object_type: "pull_request_review",
            remote_object_id: item.id.to_string(),
            url: item.html_url,
            actor: Some(actor(&item.user)),
            body: item.body,
            created_at: item.submitted_at.clone(),
            updated_at: item.submitted_at,
            evidence: "GitHub REST pull-request review submission observation",
            context: ReviewSocialContext {
                review_state: Some(item.state),
                commit_id: item.commit_id,
                ..ReviewSocialContext::default()
            },
        };
        report.review_submission_snapshots_archived += usize::from(archive_snapshot(
            root,
            remote_name,
            canonical_id,
            pull_request_number,
            observed_at,
            snapshot,
        )?);
    }
    report.disappearances_archived += archive_missing(
        root,
        remote_name,
        canonical_id,
        pull_request_number,
        observed_at,
        "review-submission",
        &submission_ids,
    )?;

    let inline_comments = fetch_inline_comments(adapter, pull_request_number)?;
    let inline_ids = inline_comments
        .iter()
        .map(|item| item.id.to_string())
        .collect::<BTreeSet<_>>();
    for item in inline_comments {
        let snapshot = SnapshotInput {
            kind_key: "inline-comment",
            remote_object_type: "pull_request_review_comment",
            remote_object_id: item.id.to_string(),
            url: item.html_url,
            actor: Some(actor(&item.user)),
            body: item.body,
            created_at: Some(item.created_at),
            updated_at: Some(item.updated_at),
            evidence: "GitHub REST pull-request inline review comment observation",
            context: ReviewSocialContext {
                pull_request_review_id: item.pull_request_review_id,
                in_reply_to_id: item.in_reply_to_id,
                commit_id: Some(item.commit_id),
                original_commit_id: Some(item.original_commit_id),
                path: Some(item.path),
                line: item.line,
                side: item.side,
                start_line: item.start_line,
                start_side: item.start_side,
                original_line: item.original_line,
                original_start_line: item.original_start_line,
                ..ReviewSocialContext::default()
            },
        };
        report.inline_comment_snapshots_archived += usize::from(archive_snapshot(
            root,
            remote_name,
            canonical_id,
            pull_request_number,
            observed_at,
            snapshot,
        )?);
    }
    report.disappearances_archived += archive_missing(
        root,
        remote_name,
        canonical_id,
        pull_request_number,
        observed_at,
        "inline-comment",
        &inline_ids,
    )?;

    let threads = fetch_review_threads(adapter, pull_request_number)?;
    let thread_ids = threads
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    for item in threads {
        let snapshot = SnapshotInput {
            kind_key: "inline-thread",
            remote_object_type: "pull_request_review_thread",
            remote_object_id: item.id,
            url: format!(
                "https://github.com/{}/pull/{pull_request_number}",
                adapter.repository
            ),
            actor: None,
            body: None,
            created_at: None,
            updated_at: None,
            evidence: "GitHub GraphQL pull-request review-thread observation",
            context: ReviewSocialContext {
                path: Some(item.path),
                line: item.line,
                side: item.diff_side,
                start_line: item.start_line,
                original_line: item.original_line,
                original_start_line: item.original_start_line,
                thread_resolved: Some(item.is_resolved),
                thread_outdated: Some(item.is_outdated),
                anchor_comment_id: item.comments.nodes.first().and_then(|node| node.database_id),
                ..ReviewSocialContext::default()
            },
        };
        report.thread_snapshots_archived += usize::from(archive_snapshot(
            root,
            remote_name,
            canonical_id,
            pull_request_number,
            observed_at,
            snapshot,
        )?);
    }
    report.disappearances_archived += archive_missing(
        root,
        remote_name,
        canonical_id,
        pull_request_number,
        observed_at,
        "inline-thread",
        &thread_ids,
    )?;

    Ok(report)
}

fn actor(user: &ApiActor) -> IncomingActor {
    IncomingActor {
        remote_id: user.id.to_string(),
        login: user.login.clone(),
    }
}

fn fetch_conversation_comments(
    adapter: &GitHubAdapter,
    pull_request_number: u64,
) -> Result<Vec<ApiConversationComment>, String> {
    paginated_get(adapter, |page| {
        format!(
            "/repos/{}/issues/{pull_request_number}/comments?per_page=100&page={page}",
            adapter.repository
        )
    })
}

fn fetch_review_submissions(
    adapter: &GitHubAdapter,
    pull_request_number: u64,
) -> Result<Vec<ApiReviewSubmission>, String> {
    paginated_get(adapter, |page| {
        format!(
            "/repos/{}/pulls/{pull_request_number}/reviews?per_page=100&page={page}",
            adapter.repository
        )
    })
}

fn fetch_inline_comments(
    adapter: &GitHubAdapter,
    pull_request_number: u64,
) -> Result<Vec<ApiInlineComment>, String> {
    paginated_get(adapter, |page| {
        format!(
            "/repos/{}/pulls/{pull_request_number}/comments?per_page=100&page={page}",
            adapter.repository
        )
    })
}

fn paginated_get<T, F>(adapter: &GitHubAdapter, path: F) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
    F: Fn(usize) -> String,
{
    let mut page = 1;
    let mut items = Vec::new();
    loop {
        let batch: Vec<T> = adapter.get(&path(page))?;
        let count = batch.len();
        items.extend(batch);
        if count < 100 {
            return Ok(items);
        }
        page += 1;
    }
}

fn fetch_review_threads(
    adapter: &GitHubAdapter,
    pull_request_number: u64,
) -> Result<Vec<GraphThread>, String> {
    let (owner, name) = adapter.repository.split_once('/').ok_or_else(|| {
        format!(
            "GitHub repository {:?} is not in owner/name form",
            adapter.repository
        )
    })?;
    let query = r#"
query($owner: String!, $name: String!, $number: Int!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $cursor) {
        nodes {
          id
          isResolved
          isOutdated
          path
          line
          startLine
          diffSide
          originalLine
          originalStartLine
          comments(first: 1) { nodes { databaseId } }
        }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"#;
    let number = i64::try_from(pull_request_number)
        .map_err(|_| format!("pull request number {pull_request_number} exceeds GraphQL Int range"))?;
    let mut cursor: Option<String> = None;
    let mut threads = Vec::new();
    loop {
        let envelope: GraphEnvelope = adapter.post(
            "/graphql",
            &json!({
                "query": query,
                "variables": {
                    "owner": owner,
                    "name": name,
                    "number": number,
                    "cursor": cursor,
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
            return Err(format!("GitHub GraphQL review-thread query failed: {messages}"));
        }
        let connection = envelope
            .data
            .and_then(|data| data.repository)
            .and_then(|repository| repository.pull_request)
            .map(|pull_request| pull_request.review_threads)
            .ok_or_else(|| {
                format!(
                    "GitHub GraphQL returned no pull request {pull_request_number} for {}",
                    adapter.repository
                )
            })?;
        threads.extend(connection.nodes);
        if !connection.page_info.has_next_page {
            return Ok(threads);
        }
        cursor = connection.page_info.end_cursor;
        if cursor.is_none() {
            return Err("GitHub GraphQL review-thread pagination claimed a next page without an end cursor".into());
        }
    }
}

fn archive_snapshot(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    pull_request_number: u64,
    observed_at: &str,
    input: SnapshotInput,
) -> Result<bool, String> {
    let actor = input.actor.clone();
    let source = IncomingSource {
        remote_object_type: input.remote_object_type.into(),
        remote_object_id: input.remote_object_id.clone(),
        url: input.url.clone(),
        created_at: input.created_at.clone(),
        updated_at: input.updated_at.clone(),
    };
    let fingerprint_material = serde_json::to_string(&(
        input.kind_key,
        &input.remote_object_id,
        &actor,
        &source,
        &input.context,
        &input.body,
    ))
    .map_err(|error| format!("could not fingerprint GitHub review social snapshot: {error}"))?;
    let fingerprint = stable_fingerprint(&fingerprint_material);
    let observed_path = observed_social_path(
        root,
        remote_name,
        canonical_id,
        input.kind_key,
        &input.remote_object_id,
    );
    if let Some(previous) = load_observed_social(&observed_path)? {
        if previous.fingerprint == fingerprint && previous.present {
            return Ok(false);
        }
    }

    let event_id = format!(
        "{remote_name}-pull-request-{pull_request_number}-{}-{}-snapshot-{fingerprint}",
        input.kind_key,
        safe_component(&input.remote_object_id)
    );
    let directory = incoming_directory(root, remote_name, observed_at, &event_id)?;
    let event = ReviewSocialEvent {
        schema: REVIEW_SOCIAL_EVENT_SCHEMA_V0.into(),
        id: event_id,
        remote: remote_name.into(),
        kind: format!("review.{}.observed_snapshot", input.kind_key.replace('-', "_")),
        observed_at: observed_at.into(),
        evidence: input.evidence.into(),
        target: IncomingTarget {
            canonical_id: canonical_id.into(),
            remote_type: "pull_request".into(),
            remote_id: pull_request_number.to_string(),
        },
        actor: actor.clone(),
        source,
        context: input.context,
    };
    let created = write_event_if_new(&directory, &event, input.body.as_deref())?;
    let observed = ObservedReviewSocial {
        schema: OBSERVED_REVIEW_SOCIAL_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        pull_request_number,
        kind_key: input.kind_key.into(),
        remote_object_id: input.remote_object_id,
        fingerprint,
        url: input.url,
        actor,
        present: true,
        observed_at: observed_at.into(),
    };
    write_toml(observed_path, &observed)?;
    Ok(created)
}

fn write_event_if_new(
    directory: &Path,
    event: &ReviewSocialEvent,
    body: Option<&str>,
) -> Result<bool, String> {
    let event_path = directory.join("event.toml");
    let body_path = directory.join("body.md");
    if directory.exists() {
        let existing_text = fs::read_to_string(&event_path)
            .map_err(|error| format!("{}: {error}", event_path.display()))?;
        let existing: ReviewSocialEvent = toml::from_str(&existing_text)
            .map_err(|error| format!("{}: {error}", event_path.display()))?;
        let same_durable_snapshot = existing.schema == event.schema
            && existing.id == event.id
            && existing.remote == event.remote
            && existing.kind == event.kind
            && existing.evidence == event.evidence
            && existing.target == event.target
            && existing.actor == event.actor
            && existing.source == event.source
            && existing.context == event.context;
        let existing_body = if body.is_some() {
            Some(
                fs::read_to_string(&body_path)
                    .map_err(|error| format!("{}: {error}", body_path.display()))?,
            )
        } else {
            None
        };
        if same_durable_snapshot && existing_body.as_deref() == body {
            return Ok(false);
        }
        return Err(format!(
            "GitHub review social snapshot collision at {}; fingerprint points to different evidence",
            directory.display()
        ));
    }
    fs::create_dir_all(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    write_toml(&event_path, event)?;
    if let Some(body) = body {
        fs::write(&body_path, body).map_err(|error| format!("{}: {error}", body_path.display()))?;
    }
    Ok(true)
}

fn archive_missing(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    pull_request_number: u64,
    observed_at: &str,
    kind_key: &str,
    current_ids: &BTreeSet<String>,
) -> Result<usize, String> {
    let directory = observed_social_directory(root, remote_name, canonical_id);
    if !directory.exists() {
        return Ok(0);
    }
    let mut archived = 0;
    for entry in fs::read_dir(&directory).map_err(|error| format!("{}: {error}", directory.display()))? {
        let path = entry
            .map_err(|error| format!("{}: {error}", directory.display()))?
            .path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let Some(mut previous) = load_observed_social(&path)? else {
            continue;
        };
        if previous.kind_key != kind_key
            || !previous.present
            || current_ids.contains(&previous.remote_object_id)
        {
            continue;
        }
        let event_id = format!(
            "{remote_name}-pull-request-{pull_request_number}-{kind_key}-{}-disappeared-{}",
            safe_component(&previous.remote_object_id),
            safe_component(observed_at)
        );
        let event_directory = incoming_directory(root, remote_name, observed_at, &event_id)?;
        let event = ReviewSocialAbsenceEvent {
            schema: REVIEW_SOCIAL_ABSENCE_SCHEMA_V0.into(),
            id: event_id,
            remote: remote_name.into(),
            kind: format!("review.{}.disappeared", kind_key.replace('-', "_")),
            observed_at: observed_at.into(),
            evidence: "The provider object existed in a prior complete paginated observation and is now absent. This archives disappearance evidence without asserting why GitHub no longer returns it.".into(),
            target: IncomingTarget {
                canonical_id: canonical_id.into(),
                remote_type: "pull_request".into(),
                remote_id: pull_request_number.to_string(),
            },
            source_kind: kind_key.into(),
            source_remote_object_id: previous.remote_object_id.clone(),
            source_url: previous.url.clone(),
            last_known_fingerprint: previous.fingerprint.clone(),
            last_known_actor: previous.actor.clone(),
        };
        if !event_directory.exists() {
            fs::create_dir_all(&event_directory)
                .map_err(|error| format!("{}: {error}", event_directory.display()))?;
            write_toml(event_directory.join("event.toml"), &event)?;
            archived += 1;
        }
        previous.present = false;
        previous.observed_at = observed_at.into();
        write_toml(&path, &previous)?;
    }
    Ok(archived)
}

fn observed_social_directory(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/reviews")
        .join(canonical_id)
        .join("social")
}

fn observed_social_path(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    kind_key: &str,
    remote_object_id: &str,
) -> PathBuf {
    observed_social_directory(root, remote_name, canonical_id).join(format!(
        "{}-{}.toml",
        safe_component(kind_key),
        safe_component(remote_object_id)
    ))
}

fn load_observed_social(path: &Path) -> Result<Option<ObservedReviewSocial>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedReviewSocial =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_REVIEW_SOCIAL_SCHEMA_V0 {
        return Err(format!("{}: unsupported review social observation schema", path.display()));
    }
    Ok(Some(observed))
}

fn safe_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn stable_fingerprint(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn social_snapshot_is_idempotent_and_changes_append() {
        let root = test_root("snapshots");
        let first = sample_snapshot("first body", "2026-09-16T10:00:00Z");
        assert!(archive_snapshot(&root, "github", "review-0001", 7, "2026-09-16T10:01:00Z", first).unwrap());
        let same = sample_snapshot("first body", "2026-09-16T10:00:00Z");
        assert!(!archive_snapshot(&root, "github", "review-0001", 7, "2026-09-16T10:02:00Z", same).unwrap());
        let edited = sample_snapshot("edited body", "2026-09-16T10:03:00Z");
        assert!(archive_snapshot(&root, "github", "review-0001", 7, "2026-09-16T10:04:00Z", edited).unwrap());
        let incoming = root.join(".project/remotes/github/incoming/2026/09");
        assert_eq!(fs::read_dir(incoming).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disappearance_is_archived_once() {
        let root = test_root("missing");
        let first = sample_snapshot("body", "2026-09-16T10:00:00Z");
        archive_snapshot(&root, "github", "review-0001", 7, "2026-09-16T10:01:00Z", first).unwrap();
        let empty = BTreeSet::new();
        assert_eq!(archive_missing(&root, "github", "review-0001", 7, "2026-09-16T10:05:00Z", "conversation-comment", &empty).unwrap(), 1);
        assert_eq!(archive_missing(&root, "github", "review-0001", 7, "2026-09-16T10:06:00Z", "conversation-comment", &empty).unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fingerprint_is_stable_and_content_sensitive() {
        assert_eq!(stable_fingerprint("abc"), stable_fingerprint("abc"));
        assert_ne!(stable_fingerprint("abc"), stable_fingerprint("abd"));
    }

    fn sample_snapshot(body: &str, updated_at: &str) -> SnapshotInput {
        SnapshotInput {
            kind_key: "conversation-comment",
            remote_object_type: "pull_request_conversation_comment",
            remote_object_id: "99".into(),
            url: "https://github.com/owner/repo/pull/7#issuecomment-99".into(),
            actor: Some(IncomingActor {
                remote_id: "1".into(),
                login: "user".into(),
            }),
            body: Some(body.into()),
            created_at: Some("2026-09-16T09:00:00Z".into()),
            updated_at: Some(updated_at.into()),
            evidence: "test",
            context: ReviewSocialContext::default(),
        }
    }

    fn test_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-review-social-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
'''


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'{label}: expected exactly one anchor, found {count}')
    return text.replace(old, new, 1)

text = lib.read_text()
text = replace_once(
    text,
    'use allodium_core::github::{\n',
    'mod review_ingress;\n\nuse allodium_core::github::{\n',
    'module declaration',
)
text = replace_once(
    text,
    '    pub reviews_observed: usize,\n    pub comments_archived: usize,\n',
    '    pub reviews_observed: usize,\n    pub review_conversation_comment_snapshots_archived: usize,\n    pub review_submission_snapshots_archived: usize,\n    pub review_inline_comment_snapshots_archived: usize,\n    pub review_thread_snapshots_archived: usize,\n    pub review_social_disappearances_archived: usize,\n    pub comments_archived: usize,\n',
    'observe report counters',
)
text = replace_once(
    text,
    '            write_observed_review(root, remote_name, &canonical_id, &review, &observed_at)?;\n            report.reviews_observed += 1;\n',
    '            write_observed_review(root, remote_name, &canonical_id, &review, &observed_at)?;\n            report.reviews_observed += 1;\n            let social = review_ingress::archive_review_social(\n                self,\n                root,\n                remote_name,\n                &canonical_id,\n                mapping.number,\n                &observed_at,\n            )?;\n            report.review_conversation_comment_snapshots_archived +=\n                social.conversation_comment_snapshots_archived;\n            report.review_submission_snapshots_archived +=\n                social.review_submission_snapshots_archived;\n            report.review_inline_comment_snapshots_archived +=\n                social.inline_comment_snapshots_archived;\n            report.review_thread_snapshots_archived += social.thread_snapshots_archived;\n            report.review_social_disappearances_archived += social.disappearances_archived;\n',
    'review social observation hook',
)
text = replace_once(
    text,
    '            return Err(format!("GitHub API returned {status}: {body}"));\n',
    '            return Err(github_api_error(status, &body));\n',
    'provider error mapping',
)
text = replace_once(
    text,
    'fn github_token_from_environment() -> Option<String> {\n',
    '''fn github_api_error(status: reqwest::StatusCode, body: &str) -> String {
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
''',
    'provider diagnostic helper',
)
text = replace_once(
    text,
    '#[cfg(test)]\nmod tests {\n',
    '''#[cfg(test)]
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
''',
    'diagnostic test module',
)
lib.write_text(text)
module.write_text(module_source)

cli_text = cli.read_text()
cli_text = replace_once(
    cli_text,
    '            println!("observed {} GitHub review(s)", report.reviews_observed);\n            println!("archived {} new comment(s)", report.comments_archived);\n',
    '''            println!("observed {} GitHub review(s)", report.reviews_observed);
            println!(
                "archived {} PR conversation-comment snapshot(s)",
                report.review_conversation_comment_snapshots_archived
            );
            println!(
                "archived {} PR review-submission snapshot(s)",
                report.review_submission_snapshots_archived
            );
            println!(
                "archived {} PR inline-comment snapshot(s)",
                report.review_inline_comment_snapshots_archived
            );
            println!(
                "archived {} PR inline-thread snapshot(s)",
                report.review_thread_snapshots_archived
            );
            println!(
                "archived {} PR social disappearance(s)",
                report.review_social_disappearances_archived
            );
            println!("archived {} new comment(s)", report.comments_archived);
''',
    'CLI review ingress counters',
)
cli.write_text(cli_text)

workflow_text = workflow.read_text()
workflow_text = replace_once(
    workflow_text,
    '  pull_request_target:\n    types: [opened, edited, closed, reopened, synchronize, ready_for_review, converted_to_draft]\n  workflow_dispatch:\n',
    '  pull_request_target:\n    types: [opened, edited, closed, reopened, synchronize, ready_for_review, converted_to_draft]\n  pull_request_review:\n    types: [submitted, edited, dismissed]\n  pull_request_review_comment:\n    types: [created, edited, deleted]\n  workflow_dispatch:\n',
    'review social workflow triggers',
)
workflow.write_text(workflow_text)
