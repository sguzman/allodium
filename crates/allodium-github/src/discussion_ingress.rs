use super::{GitHubAdapter, incoming_directory, write_toml};
use allodium_core::github::{IncomingSource, IncomingTarget};
use allodium_core::github_discussion::DiscussionMapping;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

const DISCUSSION_SOCIAL_SCHEMA_V0: &str = "allodium.github.discussion-social/v0";
const DISCUSSION_SOCIAL_EVENT_SCHEMA_V0: &str =
    "allodium.github.discussion-social-snapshot-event/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct DiscussionSocialState {
    pub schema: String,
    pub canonical_id: String,
    pub number: u64,
    pub node_id: String,
    pub url: String,
    pub root: DiscussionSocialRoot,
    #[serde(default)]
    pub comments: Vec<DiscussionSocialComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct DiscussionSocialRoot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<SocialActor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<SocialActor>,
    pub author_association: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_edited_at: Option<String>,
    pub locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_lock_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    pub upvote_count: i64,
    #[serde(default)]
    pub reaction_groups: Vec<SocialReactionGroup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<SocialAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<SocialPoll>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SocialActor {
    pub actor_type: String,
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SocialReactionGroup {
    pub content: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SocialAnswer {
    pub comment_node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_database_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen_by: Option<SocialActor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SocialPoll {
    pub node_id: String,
    pub question: String,
    pub total_vote_count: i64,
    #[serde(default)]
    pub options: Vec<SocialPollOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SocialPollOption {
    pub node_id: String,
    pub option: String,
    pub total_vote_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct DiscussionSocialComment {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_id: Option<i64>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<SocialActor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<SocialActor>,
    pub author_association: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_edited_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    pub is_answer: bool,
    pub is_minimized: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimized_reason: Option<String>,
    pub upvote_count: i64,
    #[serde(default)]
    pub reaction_groups: Vec<SocialReactionGroup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DiscussionSocialEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    target: IncomingTarget,
    source: IncomingSource,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphEnvelope<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphError>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphError {
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphSocialData {
    repository: Option<GraphSocialRepository>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphSocialRepository {
    discussion: Option<GraphSocialDiscussion>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphSocialDiscussion {
    id: String,
    number: i64,
    author: Option<GraphActor>,
    editor: Option<GraphActor>,
    #[serde(rename = "authorAssociation")]
    author_association: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    #[serde(rename = "lastEditedAt")]
    last_edited_at: Option<String>,
    locked: bool,
    #[serde(rename = "activeLockReason")]
    active_lock_reason: Option<String>,
    #[serde(rename = "stateReason")]
    state_reason: Option<String>,
    #[serde(rename = "closedAt")]
    closed_at: Option<String>,
    #[serde(rename = "upvoteCount")]
    upvote_count: i64,
    #[serde(rename = "reactionGroups", default)]
    reaction_groups: Vec<GraphReactionGroup>,
    answer: Option<GraphAnswerComment>,
    #[serde(rename = "answerChosenAt")]
    answer_chosen_at: Option<String>,
    #[serde(rename = "answerChosenBy")]
    answer_chosen_by: Option<GraphActor>,
    poll: Option<GraphPoll>,
    comments: GraphTopCommentConnection,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphActor {
    #[serde(rename = "__typename")]
    actor_type: String,
    login: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphReactionGroup {
    content: String,
    users: GraphCountConnection,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCountConnection {
    #[serde(rename = "totalCount")]
    total_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphAnswerComment {
    id: String,
    #[serde(rename = "databaseId")]
    database_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPoll {
    id: String,
    question: String,
    #[serde(rename = "totalVoteCount")]
    total_vote_count: i64,
    options: GraphPollOptionConnection,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPollOptionConnection {
    nodes: Vec<GraphPollOption>,
    #[serde(rename = "pageInfo")]
    page_info: GraphPageInfo,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPollOption {
    id: String,
    option: String,
    #[serde(rename = "totalVoteCount")]
    total_vote_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphTopCommentConnection {
    nodes: Vec<GraphTopComment>,
    #[serde(rename = "pageInfo")]
    page_info: GraphPageInfo,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphTopComment {
    #[serde(flatten)]
    comment: GraphComment,
    replies: GraphReplyConnection,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphReplyConnection {
    nodes: Vec<GraphComment>,
    #[serde(rename = "pageInfo")]
    page_info: GraphPageInfo,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphComment {
    id: String,
    #[serde(rename = "databaseId")]
    database_id: Option<i64>,
    url: String,
    author: Option<GraphActor>,
    editor: Option<GraphActor>,
    #[serde(rename = "authorAssociation")]
    author_association: String,
    body: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    #[serde(rename = "lastEditedAt")]
    last_edited_at: Option<String>,
    #[serde(rename = "deletedAt")]
    deleted_at: Option<String>,
    #[serde(rename = "isAnswer")]
    is_answer: bool,
    #[serde(rename = "isMinimized")]
    is_minimized: bool,
    #[serde(rename = "minimizedReason")]
    minimized_reason: Option<String>,
    #[serde(rename = "upvoteCount")]
    upvote_count: i64,
    #[serde(rename = "reactionGroups", default)]
    reaction_groups: Vec<GraphReactionGroup>,
    #[serde(rename = "replyTo")]
    reply_to: Option<GraphReplyTo>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphReplyTo {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphRepliesData {
    node: Option<GraphRepliesNode>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphRepliesNode {
    replies: GraphReplyConnection,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPollOptionsData {
    node: Option<GraphPollOptionsNode>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPollOptionsNode {
    options: GraphPollOptionConnection,
}

pub(super) fn archive_discussion_social(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    mapping: &DiscussionMapping,
    observed_at: &str,
) -> Result<usize, String> {
    let state = fetch_social_state(adapter, canonical_id, mapping)?;
    record_social_snapshot(root, remote_name, state, observed_at)
}

fn fetch_social_state(
    adapter: &GitHubAdapter,
    canonical_id: &str,
    mapping: &DiscussionMapping,
) -> Result<DiscussionSocialState, String> {
    let (owner, name) = repository_parts(adapter)?;
    let number = i32::try_from(mapping.number).map_err(|_| {
        format!(
            "GitHub Discussion number {} exceeds GraphQL Int range",
            mapping.number
        )
    })?;
    let mut cursor: Option<String> = None;
    let mut root: Option<DiscussionSocialRoot> = None;
    let mut comments = Vec::new();

    loop {
        let query = social_query();
        let data: GraphSocialData = graph_data(
            adapter,
            &json!({
                "query": query,
                "variables": {
                    "owner": owner,
                    "name": name,
                    "number": number,
                    "cursor": cursor,
                }
            }),
            "Discussion social query",
        )?;
        let discussion = data
            .repository
            .and_then(|repository| repository.discussion)
            .ok_or_else(|| {
                format!(
                    "GitHub GraphQL returned no Discussion #{} while reading social state for {canonical_id:?}",
                    mapping.number
                )
            })?;
        assert_identity(mapping, &discussion)?;

        if root.is_none() {
            root = Some(root_from_graph(adapter, &discussion)?);
        }

        let page_info = discussion.comments.page_info.clone();
        for top in discussion.comments.nodes {
            let top_id = top.comment.id.clone();
            comments.push(comment_from_graph(top.comment));
            for reply in top.replies.nodes {
                comments.push(comment_from_graph(reply));
            }
            if top.replies.page_info.has_next_page {
                let mut reply_cursor = top.replies.page_info.end_cursor;
                loop {
                    let page = fetch_more_replies(adapter, &top_id, reply_cursor.clone())?;
                    let reply_page_info = page.page_info.clone();
                    for reply in page.nodes {
                        comments.push(comment_from_graph(reply));
                    }
                    if !reply_page_info.has_next_page {
                        break;
                    }
                    reply_cursor = Some(required_cursor(
                        reply_page_info.end_cursor,
                        "Discussion reply pagination",
                    )?);
                }
            }
        }

        if !page_info.has_next_page {
            break;
        }
        cursor = Some(required_cursor(
            page_info.end_cursor,
            "Discussion comment pagination",
        )?);
    }

    comments.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    comments.dedup_by(|left, right| left.node_id == right.node_id);

    Ok(DiscussionSocialState {
        schema: DISCUSSION_SOCIAL_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        number: mapping.number,
        node_id: mapping.node_id.clone(),
        url: mapping.url.clone(),
        root: root
            .ok_or_else(|| "GitHub Discussion social query produced no root state".to_string())?,
        comments,
    })
}

fn root_from_graph(
    adapter: &GitHubAdapter,
    discussion: &GraphSocialDiscussion,
) -> Result<DiscussionSocialRoot, String> {
    let mut reaction_groups = reaction_groups(&discussion.reaction_groups);
    reaction_groups.sort_by(|left, right| left.content.cmp(&right.content));
    let answer = discussion.answer.as_ref().map(|answer| SocialAnswer {
        comment_node_id: answer.id.clone(),
        comment_database_id: answer.database_id,
        chosen_at: discussion.answer_chosen_at.clone(),
        chosen_by: discussion.answer_chosen_by.as_ref().map(actor),
    });
    let poll = match &discussion.poll {
        Some(poll) => {
            let mut options = poll
                .options
                .nodes
                .iter()
                .map(poll_option)
                .collect::<Vec<_>>();
            if poll.options.page_info.has_next_page {
                let mut cursor = poll.options.page_info.end_cursor.clone();
                loop {
                    let page = fetch_more_poll_options(adapter, &poll.id, cursor.clone())?;
                    let page_info = page.page_info.clone();
                    options.extend(page.nodes.iter().map(poll_option));
                    if !page_info.has_next_page {
                        break;
                    }
                    cursor = Some(required_cursor(
                        page_info.end_cursor,
                        "Discussion poll-option pagination",
                    )?);
                }
            }
            options.sort_by(|left, right| left.node_id.cmp(&right.node_id));
            options.dedup_by(|left, right| left.node_id == right.node_id);
            Some(SocialPoll {
                node_id: poll.id.clone(),
                question: poll.question.clone(),
                total_vote_count: poll.total_vote_count,
                options,
            })
        }
        None => None,
    };

    Ok(DiscussionSocialRoot {
        author: discussion.author.as_ref().map(actor),
        editor: discussion.editor.as_ref().map(actor),
        author_association: discussion.author_association.clone(),
        created_at: discussion.created_at.clone(),
        updated_at: discussion.updated_at.clone(),
        last_edited_at: discussion.last_edited_at.clone(),
        locked: discussion.locked,
        active_lock_reason: discussion.active_lock_reason.clone(),
        state_reason: discussion.state_reason.clone(),
        closed_at: discussion.closed_at.clone(),
        upvote_count: discussion.upvote_count,
        reaction_groups,
        answer,
        poll,
    })
}

fn comment_from_graph(comment: GraphComment) -> DiscussionSocialComment {
    let mut reaction_groups = reaction_groups(&comment.reaction_groups);
    reaction_groups.sort_by(|left, right| left.content.cmp(&right.content));
    DiscussionSocialComment {
        node_id: comment.id,
        database_id: comment.database_id,
        url: comment.url,
        author: comment.author.as_ref().map(actor),
        editor: comment.editor.as_ref().map(actor),
        author_association: comment.author_association,
        body: comment.body,
        created_at: comment.created_at,
        updated_at: comment.updated_at,
        last_edited_at: comment.last_edited_at,
        deleted_at: comment.deleted_at,
        is_answer: comment.is_answer,
        is_minimized: comment.is_minimized,
        minimized_reason: comment.minimized_reason,
        upvote_count: comment.upvote_count,
        reaction_groups,
        reply_to_node_id: comment.reply_to.map(|reply_to| reply_to.id),
    }
}

fn actor(actor: &GraphActor) -> SocialActor {
    SocialActor {
        actor_type: actor.actor_type.clone(),
        login: actor.login.clone(),
    }
}

fn reaction_groups(groups: &[GraphReactionGroup]) -> Vec<SocialReactionGroup> {
    groups
        .iter()
        .map(|group| SocialReactionGroup {
            content: group.content.clone(),
            count: group.users.total_count,
        })
        .collect()
}

fn poll_option(option: &GraphPollOption) -> SocialPollOption {
    SocialPollOption {
        node_id: option.id.clone(),
        option: option.option.clone(),
        total_vote_count: option.total_vote_count,
    }
}

fn fetch_more_replies(
    adapter: &GitHubAdapter,
    comment_id: &str,
    cursor: Option<String>,
) -> Result<GraphReplyConnection, String> {
    let query = format!(
        r#"
query DiscussionReplies($id: ID!, $cursor: String) {{
  node(id: $id) {{
    ... on DiscussionComment {{
      replies(first: 100, after: $cursor) {{
        nodes {{ {} }}
        pageInfo {{ hasNextPage endCursor }}
      }}
    }}
  }}
}}
"#,
        comment_fields()
    );
    let data: GraphRepliesData = graph_data(
        adapter,
        &json!({
            "query": query,
            "variables": { "id": comment_id, "cursor": cursor }
        }),
        "Discussion reply pagination query",
    )?;
    data.node
        .map(|node| node.replies)
        .ok_or_else(|| format!("GitHub GraphQL returned no DiscussionComment node {comment_id:?}"))
}

fn fetch_more_poll_options(
    adapter: &GitHubAdapter,
    poll_id: &str,
    cursor: Option<String>,
) -> Result<GraphPollOptionConnection, String> {
    let query = r#"
query DiscussionPollOptions($id: ID!, $cursor: String) {
  node(id: $id) {
    ... on DiscussionPoll {
      options(first: 100, after: $cursor) {
        nodes { id option totalVoteCount }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"#;
    let data: GraphPollOptionsData = graph_data(
        adapter,
        &json!({
            "query": query,
            "variables": { "id": poll_id, "cursor": cursor }
        }),
        "Discussion poll-option pagination query",
    )?;
    data.node
        .map(|node| node.options)
        .ok_or_else(|| format!("GitHub GraphQL returned no DiscussionPoll node {poll_id:?}"))
}

fn social_query() -> String {
    format!(
        r#"
query DiscussionSocial($owner: String!, $name: String!, $number: Int!, $cursor: String) {{
  repository(owner: $owner, name: $name) {{
    discussion(number: $number) {{
      id
      number
      url
      author {{ __typename login }}
      editor {{ __typename login }}
      authorAssociation
      createdAt
      updatedAt
      lastEditedAt
      locked
      activeLockReason
      stateReason
      closedAt
      upvoteCount
      reactionGroups {{ content users {{ totalCount }} }}
      answer {{ id databaseId }}
      answerChosenAt
      answerChosenBy {{ __typename login }}
      poll {{
        id
        question
        totalVoteCount
        options(first: 100) {{
          nodes {{ id option totalVoteCount }}
          pageInfo {{ hasNextPage endCursor }}
        }}
      }}
      comments(first: 100, after: $cursor) {{
        nodes {{
          {}
          replies(first: 100) {{
            nodes {{ {} }}
            pageInfo {{ hasNextPage endCursor }}
          }}
        }}
        pageInfo {{ hasNextPage endCursor }}
      }}
    }}
  }}
}}
"#,
        comment_fields(),
        comment_fields()
    )
}

fn comment_fields() -> &'static str {
    r#"
          id
          databaseId
          url
          author { __typename login }
          editor { __typename login }
          authorAssociation
          body
          createdAt
          updatedAt
          lastEditedAt
          deletedAt
          isAnswer
          isMinimized
          minimizedReason
          upvoteCount
          reactionGroups { content users { totalCount } }
          replyTo { id }
"#
}

fn graph_data<T: DeserializeOwned>(
    adapter: &GitHubAdapter,
    payload: &serde_json::Value,
    context: &str,
) -> Result<T, String> {
    let envelope: GraphEnvelope<T> = adapter.post("/graphql", payload)?;
    if !envelope.errors.is_empty() {
        let messages = envelope
            .errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("GitHub GraphQL {context} failed: {messages}"));
    }
    envelope
        .data
        .ok_or_else(|| format!("GitHub GraphQL {context} returned no data"))
}

fn repository_parts(adapter: &GitHubAdapter) -> Result<(&str, &str), String> {
    adapter.repository.split_once('/').ok_or_else(|| {
        format!(
            "GitHub repository {:?} is not in owner/name form",
            adapter.repository
        )
    })
}

fn assert_identity(
    mapping: &DiscussionMapping,
    discussion: &GraphSocialDiscussion,
) -> Result<(), String> {
    let number = u64::try_from(discussion.number).map_err(|_| {
        format!(
            "GitHub Discussion returned invalid negative number {} while reading social state",
            discussion.number
        )
    })?;
    if number != mapping.number || discussion.id != mapping.node_id {
        return Err(format!(
            "GitHub Discussion social observation disagrees with stable mapping: expected #{} / {}, observed #{} / {}",
            mapping.number, mapping.node_id, number, discussion.id
        ));
    }
    Ok(())
}

fn required_cursor(cursor: Option<String>, context: &str) -> Result<String, String> {
    cursor.ok_or_else(|| format!("{context} claimed a next page without an end cursor"))
}

fn record_social_snapshot(
    root: &Path,
    remote_name: &str,
    state: DiscussionSocialState,
    observed_at: &str,
) -> Result<usize, String> {
    if state.schema != DISCUSSION_SOCIAL_SCHEMA_V0 {
        return Err(format!(
            "refusing unsupported Discussion social snapshot schema {:?}",
            state.schema
        ));
    }
    let path = observed_social_path(root, remote_name, &state.canonical_id);
    let current_json = serde_json::to_string_pretty(&state)
        .map_err(|error| format!("could not serialize GitHub Discussion social state: {error}"))?
        + "\n";
    let previous_json = if path.exists() {
        Some(fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?)
    } else {
        None
    };
    if previous_json.as_deref() == Some(current_json.as_str()) {
        return Ok(0);
    }

    let transition = format!(
        "{}\0{}",
        previous_json.as_deref().unwrap_or("<initial>"),
        current_json
    );
    let fingerprint = stable_fingerprint(&transition);
    let event_id = format!(
        "{remote_name}-discussion-{}-social-{fingerprint}",
        state.number
    );
    let directory = incoming_directory(root, remote_name, observed_at, &event_id)?;
    if !directory.exists() {
        fs::create_dir_all(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        let event = DiscussionSocialEvent {
            schema: DISCUSSION_SOCIAL_EVENT_SCHEMA_V0.into(),
            id: event_id,
            remote: remote_name.into(),
            kind: if previous_json.is_some() {
                "discussion.social.snapshot.changed".into()
            } else {
                "discussion.social.snapshot.observed".into()
            },
            observed_at: observed_at.into(),
            evidence: "Normalized GitHub GraphQL Discussion social state. Comment/discussion authors and answer chooser are provider-scoped actors. Reaction and upvote data are provider-exposed counts; Allodium does not infer individual voters from counts. This evidence never mutates canonical discussion authorship or social state.".into(),
            target: IncomingTarget {
                canonical_id: state.canonical_id.clone(),
                remote_type: "discussion".into(),
                remote_id: state.number.to_string(),
            },
            source: IncomingSource {
                remote_object_type: "discussion".into(),
                remote_object_id: state.node_id.clone(),
                created_at: Some(state.root.created_at.clone()),
                updated_at: Some(state.root.updated_at.clone()),
                url: state.url.clone(),
            },
        };
        write_toml(directory.join("event.toml"), &event)?;
        if let Some(previous) = &previous_json {
            fs::write(directory.join("before.json"), previous)
                .map_err(|error| format!("{}: {error}", directory.display()))?;
        }
        fs::write(directory.join("after.json"), &current_json)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
    }

    fs::create_dir_all(
        path.parent()
            .expect("Discussion social observation path has parent"),
    )
    .map_err(|error| format!("{}: {error}", path.display()))?;
    fs::write(&path, current_json).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(1)
}

fn observed_social_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/discussions/social")
        .join(format!("{canonical_id}.json"))
}

fn stable_fingerprint(text: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
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
    fn social_snapshot_is_archived_once_and_changes_are_preserved() {
        let root = test_root("archive");
        let first = sample_state(0);
        assert_eq!(
            record_social_snapshot(&root, "github", first.clone(), "2026-09-17T12:00:00Z").unwrap(),
            1
        );
        assert_eq!(
            record_social_snapshot(&root, "github", first.clone(), "2026-09-17T12:01:00Z").unwrap(),
            0
        );
        let mut changed = first;
        changed.comments[0].upvote_count = 1;
        changed.comments[0].reaction_groups[0].count = 2;
        changed.root.locked = true;
        changed.root.active_lock_reason = Some("OFF_TOPIC".into());
        assert_eq!(
            record_social_snapshot(&root, "github", changed, "2026-09-17T12:02:00Z").unwrap(),
            1
        );
        let incoming = root.join(".project/remotes/github/incoming/2026/09");
        assert_eq!(fs::read_dir(incoming).unwrap().count(), 2);
        let observed =
            fs::read_to_string(observed_social_path(&root, "github", "discussion-0001")).unwrap();
        assert!(observed.contains("OFF_TOPIC"));
        assert!(observed.contains("\"upvote_count\": 1"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ordering_noise_is_normalized_before_persistence() {
        let root = test_root("ordering");
        let mut first = sample_state(0);
        first.comments.push(reply_comment());
        let mut reordered = first.clone();
        reordered.comments.reverse();
        reordered.root.reaction_groups.reverse();
        normalize_state(&mut first);
        normalize_state(&mut reordered);
        assert_eq!(first, reordered);
        assert_eq!(
            record_social_snapshot(&root, "github", first, "2026-09-17T12:00:00Z").unwrap(),
            1
        );
        assert_eq!(
            record_social_snapshot(&root, "github", reordered, "2026-09-17T12:01:00Z").unwrap(),
            0
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn normalize_state(state: &mut DiscussionSocialState) {
        state
            .comments
            .sort_by(|left, right| left.node_id.cmp(&right.node_id));
        state
            .root
            .reaction_groups
            .sort_by(|left, right| left.content.cmp(&right.content));
        for comment in &mut state.comments {
            comment
                .reaction_groups
                .sort_by(|left, right| left.content.cmp(&right.content));
        }
        if let Some(poll) = &mut state.root.poll {
            poll.options
                .sort_by(|left, right| left.node_id.cmp(&right.node_id));
        }
    }

    fn sample_state(upvotes: i64) -> DiscussionSocialState {
        DiscussionSocialState {
            schema: DISCUSSION_SOCIAL_SCHEMA_V0.into(),
            canonical_id: "discussion-0001".into(),
            number: 42,
            node_id: "D_discussion".into(),
            url: "https://github.com/owner/repo/discussions/42".into(),
            root: DiscussionSocialRoot {
                author: Some(SocialActor {
                    actor_type: "User".into(),
                    login: "author".into(),
                }),
                editor: None,
                author_association: "OWNER".into(),
                created_at: "2026-09-17T10:00:00Z".into(),
                updated_at: "2026-09-17T10:00:00Z".into(),
                last_edited_at: None,
                locked: false,
                active_lock_reason: None,
                state_reason: None,
                closed_at: None,
                upvote_count: upvotes,
                reaction_groups: vec![
                    SocialReactionGroup {
                        content: "THUMBS_UP".into(),
                        count: 1,
                    },
                    SocialReactionGroup {
                        content: "HEART".into(),
                        count: 0,
                    },
                ],
                answer: Some(SocialAnswer {
                    comment_node_id: "DC_comment".into(),
                    comment_database_id: Some(77),
                    chosen_at: Some("2026-09-17T11:00:00Z".into()),
                    chosen_by: Some(SocialActor {
                        actor_type: "User".into(),
                        login: "chooser".into(),
                    }),
                }),
                poll: Some(SocialPoll {
                    node_id: "DP_poll".into(),
                    question: "Ship it?".into(),
                    total_vote_count: 3,
                    options: vec![SocialPollOption {
                        node_id: "DPO_yes".into(),
                        option: "Yes".into(),
                        total_vote_count: 3,
                    }],
                }),
            },
            comments: vec![DiscussionSocialComment {
                node_id: "DC_comment".into(),
                database_id: Some(77),
                url: "https://github.com/owner/repo/discussions/42#discussioncomment-77".into(),
                author: Some(SocialActor {
                    actor_type: "User".into(),
                    login: "commenter".into(),
                }),
                editor: None,
                author_association: "CONTRIBUTOR".into(),
                body: "A comment\n".into(),
                created_at: "2026-09-17T10:30:00Z".into(),
                updated_at: "2026-09-17T10:30:00Z".into(),
                last_edited_at: None,
                deleted_at: None,
                is_answer: true,
                is_minimized: false,
                minimized_reason: None,
                upvote_count: upvotes,
                reaction_groups: vec![SocialReactionGroup {
                    content: "THUMBS_UP".into(),
                    count: 1,
                }],
                reply_to_node_id: None,
            }],
        }
    }

    fn reply_comment() -> DiscussionSocialComment {
        DiscussionSocialComment {
            node_id: "DC_reply".into(),
            database_id: Some(78),
            url: "https://github.com/owner/repo/discussions/42#discussioncomment-78".into(),
            author: Some(SocialActor {
                actor_type: "User".into(),
                login: "replier".into(),
            }),
            editor: None,
            author_association: "NONE".into(),
            body: "Reply\n".into(),
            created_at: "2026-09-17T10:40:00Z".into(),
            updated_at: "2026-09-17T10:40:00Z".into(),
            last_edited_at: None,
            deleted_at: None,
            is_answer: false,
            is_minimized: false,
            minimized_reason: None,
            upvote_count: 0,
            reaction_groups: Vec::new(),
            reply_to_node_id: Some("DC_comment".into()),
        }
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-discussion-social-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
