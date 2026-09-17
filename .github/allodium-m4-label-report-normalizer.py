from pathlib import Path

path = Path("crates/allodium-github/src/lib.rs")
text = path.read_text()

observe_start = text.index("#[derive(Debug, Clone, Default, PartialEq, Eq)]\npub struct ObserveReport {")
apply_start = text.index("#[derive(Debug, Clone, Default, PartialEq, Eq)]\npub struct ApplyReport {", observe_start)
next_struct = text.index("#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]", apply_start)

observe = '''#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObserveReport {
    pub issues_observed: usize,
    pub reviews_observed: usize,
    pub wikis_observed: usize,
    pub wiki_remote_changes_archived: usize,
    pub releases_observed: usize,
    pub release_managed_changes_archived: usize,
    pub milestones_observed: usize,
    pub milestone_managed_changes_archived: usize,
    pub labels_observed: usize,
    pub label_managed_changes_archived: usize,
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

'''

apply = '''#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub issues_created: usize,
    pub issues_updated: usize,
    pub issues_observed: usize,
    pub reviews_created: usize,
    pub reviews_updated: usize,
    pub reviews_observed: usize,
    pub wikis_observed: usize,
    pub wikis_updated: usize,
    pub wiki_bootstrap_required: usize,
    pub releases_created: usize,
    pub releases_updated: usize,
    pub releases_observed: usize,
    pub milestones_created: usize,
    pub milestones_updated: usize,
    pub milestones_observed: usize,
    pub labels_created: usize,
    pub labels_updated: usize,
    pub labels_observed: usize,
}

'''

text = text[:observe_start] + observe + apply + text[next_struct:]
path.write_text(text)
