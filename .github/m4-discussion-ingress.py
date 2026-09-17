from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    return text.replace(old, new, 1)

social_path = Path("crates/allodium-github/src/discussion_ingress.rs")
social = social_path.read_text()
social = replace_once(
    social,
    "                created_at: state.root.created_at.clone(),\n                updated_at: state.root.updated_at.clone(),",
    "                created_at: Some(state.root.created_at.clone()),\n                updated_at: Some(state.root.updated_at.clone()),",
    "Discussion social IncomingSource timestamps",
)
social_path.write_text(social)

lib_path = Path("crates/allodium-github/src/lib.rs")
lib = lib_path.read_text()
lib = replace_once(
    lib,
    "mod discussion;\n",
    "mod discussion;\nmod discussion_ingress;\n",
    "discussion ingress module declaration",
)
lib = replace_once(
    lib,
    "    pub discussion_managed_changes_archived: usize,\n    pub review_conversation_comment_snapshots_archived: usize,",
    "    pub discussion_managed_changes_archived: usize,\n    pub discussion_social_snapshots_archived: usize,\n    pub review_conversation_comment_snapshots_archived: usize,",
    "Discussion social ObserveReport counter",
)
lib = replace_once(
    lib,
    "        report.discussion_managed_changes_archived += discussion_report.managed_changes_archived;",
    "        report.discussion_managed_changes_archived += discussion_report.managed_changes_archived;\n        report.discussion_social_snapshots_archived += discussion_report.social_snapshots_archived;",
    "Discussion social observe report integration",
)
lib_path.write_text(lib)

discussion_path = Path("crates/allodium-github/src/discussion.rs")
discussion = discussion_path.read_text()
discussion = replace_once(
    discussion,
    "    DISCUSSION_MAPPINGS_SCHEMA_V0, DiscussionMapping, DiscussionMappings,\n",
    "    DiscussionMapping,\n",
    "Discussion runtime production imports",
)
discussion = replace_once(
    discussion,
    "        DiscussionProjectionBinding, DiscussionProjectionConfig, write_observed_discussion_snapshot,\n",
    "        DISCUSSION_MAPPINGS_SCHEMA_V0, DiscussionMappings, write_observed_discussion_snapshot,\n",
    "Discussion runtime test imports",
)
discussion = replace_once(
    discussion,
    "    pub managed_changes_archived: usize,\n}",
    "    pub managed_changes_archived: usize,\n    pub social_snapshots_archived: usize,\n}",
    "DiscussionObserveReport social counter",
)
old = '''        write_observed_discussion_snapshot(root, remote_name, &snapshot)?;
        report.observed += 1;
'''
new = '''        write_observed_discussion_snapshot(root, remote_name, &snapshot)?;
        report.observed += 1;
        report.social_snapshots_archived += super::discussion_ingress::archive_discussion_social(
            adapter,
            root,
            remote_name,
            &canonical_id,
            &mapping,
            &now(),
        )?;
'''
discussion = replace_once(
    discussion,
    old,
    new,
    "mapped Discussion social archive hook",
)
discussion_path.write_text(discussion)

cli_path = Path("crates/allodium-cli/src/main.rs")
cli = cli_path.read_text()
anchor = '''            println!(
                "archived {} GitHub Discussion managed-field remote change(s)",
                report.discussion_managed_changes_archived
            );
'''
replacement = anchor + '''            println!(
                "archived {} GitHub Discussion social snapshot change(s)",
                report.discussion_social_snapshots_archived
            );
'''
cli = replace_once(cli, anchor, replacement, "Discussion social CLI output")
cli_path.write_text(cli)
