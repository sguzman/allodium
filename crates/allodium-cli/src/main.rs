use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");

    match command {
        "validate" => validate(root_arg(&args, 1)),
        "inspect" => inspect(root_arg(&args, 1)),
        "github" => github(&args),
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}\n");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn validate(root: PathBuf) -> ExitCode {
    let report = allodium_core::validate(&root);
    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }
    for error in &report.errors {
        eprintln!("error: {error}");
    }
    if report.is_ok() {
        println!("Allodium project state is valid: {}", root.display());
        ExitCode::SUCCESS
    } else {
        eprintln!("validation failed with {} error(s)", report.errors.len());
        ExitCode::FAILURE
    }
}

fn inspect(root: PathBuf) -> ExitCode {
    match allodium_core::load_manifest(&root) {
        Ok(manifest) => {
            println!("{} ({})", manifest.name, manifest.id);
            if !manifest.description.is_empty() {
                println!("{}", manifest.description);
            }
            println!("schema: {}", manifest.schema);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn github(args: &[String]) -> ExitCode {
    let subcommand = args.get(1).map(String::as_str).unwrap_or("help");
    match subcommand {
        "plan" => github_plan(
            root_arg(args, 2),
            args.get(3).map(String::as_str).unwrap_or("github"),
        ),
        "observe" => github_observe(
            root_arg(args, 2),
            args.get(3).map(String::as_str).unwrap_or("github"),
        ),
        "apply" => github_apply(
            PathBuf::from(args.get(2).map(String::as_str).unwrap_or("plan.toml")),
            root_arg(args, 3),
        ),
        "help" | "--help" | "-h" => {
            print_github_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown github command: {other}\n");
            print_github_help();
            ExitCode::from(2)
        }
    }
}

fn github_plan(root: PathBuf, remote_name: &str) -> ExitCode {
    let result = allodium_core::github::plan_issues(&root, remote_name).and_then(|mut plan| {
        let review_plan = allodium_core::github::plan_reviews(&root, remote_name)?;
        if plan.remote != review_plan.remote || plan.repository != review_plan.repository {
            return Err(
                "issue and review projection plans disagree about the GitHub remote".into(),
            );
        }
        plan.operations.extend(review_plan.operations);

        let wiki_plan = allodium_core::github_wiki::plan_wiki(&root, remote_name)?;
        if plan.remote != wiki_plan.remote || plan.repository != wiki_plan.repository {
            return Err("GitHub wiki projection plan disagrees about the configured remote".into());
        }
        plan.operations.extend(wiki_plan.operations);

        let release_plan = allodium_core::github_release::plan_releases(&root, remote_name)?;
        if plan.remote != release_plan.remote || plan.repository != release_plan.repository {
            return Err(
                "GitHub Release projection plan disagrees about the configured remote".into(),
            );
        }
        plan.operations.extend(release_plan.operations);

        let milestone_plan = allodium_core::github_milestone::plan_milestones(&root, remote_name)?;
        if plan.remote != milestone_plan.remote || plan.repository != milestone_plan.repository {
            return Err(
                "GitHub milestone projection plan disagrees about the configured remote".into(),
            );
        }
        plan.operations.extend(milestone_plan.operations);

        let label_plan = allodium_core::github_label::plan_labels(&root, remote_name)?;
        if plan.remote != label_plan.remote || plan.repository != label_plan.repository {
            return Err(
                "GitHub label projection plan disagrees about the configured remote".into(),
            );
        }
        plan.operations.extend(label_plan.operations);

        let discussion_plan =
            allodium_core::github_discussion::plan_discussions(&root, remote_name)?;
        if plan.remote != discussion_plan.remote || plan.repository != discussion_plan.repository {
            return Err(
                "GitHub Discussion projection plan disagrees about the configured remote".into(),
            );
        }
        plan.operations.extend(discussion_plan.operations);

        let projects_plan = allodium_core::github_project::plan_boards(&root, remote_name)?;
        if plan.remote != projects_plan.remote || plan.repository != projects_plan.repository {
            return Err(
                "GitHub Projects projection plan disagrees about the configured remote".into(),
            );
        }
        plan.operations.extend(projects_plan.operations);
        Ok(plan)
    });

    match result.and_then(|plan| allodium_core::github::plan_to_toml(&plan)) {
        Ok(plan) => {
            print!("{plan}");
            ExitCode::SUCCESS
        }
        Err(error) => fail(error),
    }
}

fn github_observe(root: PathBuf, remote_name: &str) -> ExitCode {
    let result = allodium_github::GitHubAdapter::from_project(&root, remote_name)
        .and_then(|adapter| adapter.observe(&root, remote_name));
    match result {
        Ok(report) => {
            println!("observed {} GitHub issue(s)", report.issues_observed);
            println!("observed {} GitHub review(s)", report.reviews_observed);
            println!("observed {} GitHub wiki(s)", report.wikis_observed);
            println!("observed {} GitHub release(s)", report.releases_observed);
            println!(
                "observed {} GitHub milestone(s)",
                report.milestones_observed
            );
            println!("observed {} GitHub label(s)", report.labels_observed);
            println!(
                "observed {} GitHub Discussion capability snapshot(s)",
                report.discussion_capabilities_observed
            );
            println!(
                "observed {} mapped GitHub Discussion(s)",
                report.discussions_observed
            );
            println!(
                "observed {} GitHub Projects capability snapshot(s)",
                report.projects_capabilities_observed
            );
            println!(
                "observed {} GitHub ProjectV2 project(s)",
                report.projects_observed
            );
            println!(
                "observed {} GitHub Project content node identit(y/ies)",
                report.projects_content_identities_observed
            );
            println!(
                "mapped {} GitHub Project item identit(y/ies)",
                report.projects_item_identities_mapped
            );
            println!(
                "archived {} GitHub Project provider-state change(s)",
                report.projects_provider_changes_archived
            );
            println!(
                "archived {} GitHub Discussion managed-field remote change(s)",
                report.discussion_managed_changes_archived
            );
            println!(
                "archived {} GitHub Discussion social snapshot change(s)",
                report.discussion_social_snapshots_archived
            );
            println!(
                "archived {} GitHub Wiki remote-tree change(s)",
                report.wiki_remote_changes_archived
            );
            println!(
                "archived {} GitHub Release managed-field remote change(s)",
                report.release_managed_changes_archived
            );
            println!(
                "archived {} GitHub milestone managed-field remote change(s)",
                report.milestone_managed_changes_archived
            );
            println!(
                "archived {} GitHub label managed-field remote change(s)",
                report.label_managed_changes_archived
            );
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
            println!(
                "archived {} edited comment revision(s)",
                report.comment_edits_archived
            );
            println!(
                "archived {} missing-comment observation(s)",
                report.comment_disappearances_archived
            );
            println!(
                "archived {} issue managed-field remote change(s)",
                report.managed_changes_archived
            );
            println!(
                "archived {} review managed-field remote change(s)",
                report.review_managed_changes_archived
            );
            println!(
                "archived {} unmapped GitHub issue revision(s)",
                report.unmapped_issues_archived
            );
            ExitCode::SUCCESS
        }
        Err(error) => fail(error),
    }
}

fn github_apply(plan_path: PathBuf, root: PathBuf) -> ExitCode {
    let result = allodium_github::load_plan(&plan_path).and_then(|plan| {
        allodium_github::GitHubAdapter::from_project(&root, &plan.remote)
            .and_then(|adapter| adapter.apply(&root, &plan))
    });
    match result {
        Ok(report) => {
            println!("created {} GitHub issue(s)", report.issues_created);
            println!("updated {} GitHub issue(s)", report.issues_updated);
            println!("observed {} GitHub issue(s)", report.issues_observed);
            println!("created {} GitHub review(s)", report.reviews_created);
            println!("updated {} GitHub review(s)", report.reviews_updated);
            println!("observed {} GitHub review(s)", report.reviews_observed);
            println!("observed {} GitHub wiki(s)", report.wikis_observed);
            println!("updated {} GitHub wiki(s)", report.wikis_updated);
            println!(
                "{} GitHub wiki projection(s) require provider bootstrap",
                report.wiki_bootstrap_required
            );
            println!("created {} GitHub release(s)", report.releases_created);
            println!("updated {} GitHub release(s)", report.releases_updated);
            println!("observed {} GitHub release(s)", report.releases_observed);
            println!("created {} GitHub milestone(s)", report.milestones_created);
            println!("updated {} GitHub milestone(s)", report.milestones_updated);
            println!(
                "observed {} GitHub milestone(s)",
                report.milestones_observed
            );
            println!("created {} GitHub label(s)", report.labels_created);
            println!("updated {} GitHub label(s)", report.labels_updated);
            println!("observed {} GitHub label(s)", report.labels_observed);
            println!(
                "observed {} GitHub Discussion capability snapshot(s)",
                report.discussion_capabilities_observed
            );
            println!(
                "created {} GitHub Discussion(s)",
                report.discussions_created
            );
            println!(
                "updated {} GitHub Discussion(s)",
                report.discussions_updated
            );
            println!(
                "observed {} GitHub Discussion(s)",
                report.discussions_observed
            );
            println!("closed {} GitHub Discussion(s)", report.discussions_closed);
            println!(
                "reopened {} GitHub Discussion(s)",
                report.discussions_reopened
            );
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
            println!(
                "observed {} GitHub Projects capability snapshot(s)",
                report.projects_capabilities_observed
            );
            println!(
                "created {} GitHub ProjectV2 project(s)",
                report.projects_created
            );
            println!(
                "bound {} existing GitHub ProjectV2 project(s)",
                report.projects_bound
            );
            println!(
                "observed {} GitHub ProjectV2 project(s) during apply",
                report.projects_observed
            );
            println!(
                "updated {} GitHub ProjectV2 project(s)",
                report.projects_updated
            );
            println!(
                "created {} GitHub ProjectV2 field(s)",
                report.project_fields_created
            );
            println!(
                "updated {} GitHub ProjectV2 single-select option schema(s)",
                report.project_field_option_schemas_updated
            );
            println!(
                "added {} GitHub ProjectV2 item(s)",
                report.project_items_added
            );
            println!(
                "updated {} GitHub ProjectV2 field value(s)",
                report.project_field_values_updated
            );
            println!(
                "created {} GitHub ProjectV2 view(s)",
                report.project_views_created
            );
            println!(
                "{} GitHub Projects projection(s) require configuration",
                report.projects_config_required
            );
            println!(
                "{} GitHub Projects projection(s) require separate authorization",
                report.projects_auth_required
            );
            println!(
                "{} GitHub Projects projection(s) require owner review",
                report.projects_owner_review_required
            );
            println!(
                "{} GitHub Projects projection(s) remain at a deferred/non-mutating provider boundary",
                report.projects_runtime_required
            );
            ExitCode::SUCCESS
        }
        Err(error) => fail(error),
    }
}

fn fail(error: String) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}

fn root_arg(args: &[String], index: usize) -> PathBuf {
    PathBuf::from(args.get(index).map(String::as_str).unwrap_or("."))
}

fn print_help() {
    println!("allodium - filesystem-authoritative project state");
    println!();
    println!("USAGE:");
    println!("  allodium validate [ROOT]");
    println!("  allodium inspect  [ROOT]");
    println!("  allodium github plan [ROOT] [REMOTE]");
    println!("  allodium github observe [ROOT] [REMOTE]");
    println!("  allodium github apply PLAN [ROOT]");
}

fn print_github_help() {
    println!("allodium github - GitHub projection tools");
    println!();
    println!("USAGE:");
    println!("  allodium github plan [ROOT] [REMOTE]");
    println!("  allodium github observe [ROOT] [REMOTE]");
    println!("  allodium github apply PLAN [ROOT]");
}
