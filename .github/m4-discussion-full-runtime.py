from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    return text.replace(old, new, 1)

lib_path = Path("crates/allodium-github/src/lib.rs")
lib = lib_path.read_text()

lib = replace_once(
    lib,
    "    pub discussion_capabilities_observed: usize,\n    pub review_conversation_comment_snapshots_archived: usize,",
    "    pub discussion_capabilities_observed: usize,\n    pub discussions_observed: usize,\n    pub discussion_managed_changes_archived: usize,\n    pub review_conversation_comment_snapshots_archived: usize,",
    "ObserveReport discussion counters",
)

lib = replace_once(
    lib,
    "    pub discussion_capabilities_observed: usize,\n    pub discussions_bootstrap_required: usize,\n}",
    "    pub discussion_capabilities_observed: usize,\n    pub discussions_created: usize,\n    pub discussions_updated: usize,\n    pub discussions_observed: usize,\n    pub discussions_closed: usize,\n    pub discussions_reopened: usize,\n    pub discussions_bootstrap_required: usize,\n    pub discussion_config_required: usize,\n    pub discussion_category_required: usize,\n    pub discussion_category_review_required: usize,\n    pub discussion_category_change_unsupported: usize,\n}",
    "ApplyReport discussion counters",
)

lib = replace_once(
    lib,
    "        let discussion_report = discussion::observe_discussions(self, root, remote_name)?;\n        report.discussion_capabilities_observed += discussion_report.capabilities_observed;",
    "        let discussion_report = discussion::observe_discussions(self, root, remote_name)?;\n        report.discussion_capabilities_observed += discussion_report.capabilities_observed;\n        report.discussions_observed += discussion_report.observed;\n        report.discussion_managed_changes_archived += discussion_report.managed_changes_archived;",
    "discussion observe report integration",
)

old_dispatch = '''                "observe_discussion_capabilities" => {
                    discussion::apply_observe_discussion_capabilities(self, root, &plan.remote)?;
                    report.discussion_capabilities_observed += 1;
                }
                "discussions_bootstrap_required" => {
                    report.discussions_bootstrap_required += 1;
                }
'''
new_dispatch = '''                "observe_discussion_capabilities" => {
                    discussion::apply_observe_discussion_capabilities(self, root, &plan.remote)?;
                    report.discussion_capabilities_observed += 1;
                }
                "discussion_config_required" => {
                    report.discussion_config_required += 1;
                }
                "discussion_category_required" => {
                    report.discussion_category_required += 1;
                }
                "discussion_category_drift_requires_review" => {
                    report.discussion_category_review_required += 1;
                }
                "discussion_category_change_unsupported" => {
                    report.discussion_category_change_unsupported += 1;
                }
                "discussions_bootstrap_required" => {
                    report.discussions_bootstrap_required += 1;
                }
                "create_discussion" => {
                    discussion::apply_create_discussion(
                        self,
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                    )?;
                    report.discussions_created += 1;
                }
                "observe_discussion" => {
                    let number = require_number(operation)?;
                    discussion::apply_observe_discussion(
                        self,
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        number,
                    )?;
                    report.discussions_observed += 1;
                }
                "update_discussion" => {
                    let number = require_number(operation)?;
                    discussion::apply_update_discussion(
                        self,
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        number,
                        &operation.fields,
                    )?;
                    report.discussions_updated += 1;
                }
                "close_discussion" => {
                    let number = require_number(operation)?;
                    discussion::apply_close_discussion(
                        self,
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        number,
                    )?;
                    report.discussions_closed += 1;
                }
                "reopen_discussion" => {
                    let number = require_number(operation)?;
                    discussion::apply_reopen_discussion(
                        self,
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        number,
                    )?;
                    report.discussions_reopened += 1;
                }
'''
lib = replace_once(lib, old_dispatch, new_dispatch, "discussion apply dispatch")
lib_path.write_text(lib)

cli_path = Path("crates/allodium-cli/src/main.rs")
cli = cli_path.read_text()

observe_anchor = '''            println!(
                "observed {} GitHub Discussion capability snapshot(s)",
                report.discussion_capabilities_observed
            );
'''
observe_replacement = observe_anchor + '''            println!(
                "observed {} mapped GitHub Discussion(s)",
                report.discussions_observed
            );
            println!(
                "archived {} GitHub Discussion managed-field remote change(s)",
                report.discussion_managed_changes_archived
            );
'''
cli = replace_once(cli, observe_anchor, observe_replacement, "CLI observe discussion output")

apply_anchor = '''            println!(
                "observed {} GitHub Discussion capability snapshot(s)",
                report.discussion_capabilities_observed
            );
            println!(
                "{} GitHub Discussion projection(s) require provider bootstrap",
                report.discussions_bootstrap_required
            );
'''
apply_replacement = '''            println!(
                "observed {} GitHub Discussion capability snapshot(s)",
                report.discussion_capabilities_observed
            );
            println!("created {} GitHub Discussion(s)", report.discussions_created);
            println!("updated {} GitHub Discussion(s)", report.discussions_updated);
            println!("observed {} GitHub Discussion(s)", report.discussions_observed);
            println!("closed {} GitHub Discussion(s)", report.discussions_closed);
            println!("reopened {} GitHub Discussion(s)", report.discussions_reopened);
            println!(
                "{} GitHub Discussion projection(s) require provider bootstrap",
                report.discussions_bootstrap_required
            );
            println!(
                "{} GitHub Discussion projection(s) require projection configuration",
                report.discussion_config_required
            );
            println!(
                "{} GitHub Discussion projection(s) require a configured provider category",
                report.discussion_category_required
            );
            println!(
                "{} GitHub Discussion projection(s) have provider category drift requiring review",
                report.discussion_category_review_required
            );
            println!(
                "{} GitHub Discussion projection(s) request unsupported category reclassification",
                report.discussion_category_change_unsupported
            );
'''
cli = replace_once(cli, apply_anchor, apply_replacement, "CLI apply discussion output")
cli_path.write_text(cli)
