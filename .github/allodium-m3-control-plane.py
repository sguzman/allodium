from pathlib import Path

# Export the offline release projection model.
core = Path('crates/allodium-core/src/lib.rs')
text = core.read_text()
anchor = 'pub mod github;\npub mod github_wiki;\npub mod release;\n'
replacement = 'pub mod github;\npub mod github_release;\npub mod github_wiki;\npub mod release;\n'
if anchor not in text:
    raise SystemExit('missing core module export anchor')
core.write_text(text.replace(anchor, replacement, 1))

# Make an observed uninitialized GitHub Wiki an explicit non-mutating capability operation.
wiki = Path('crates/allodium-core/src/github_wiki.rs')
text = wiki.read_text()
old = '''        Some((_observation, observed_files)) if observed_files != desired => {
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
'''
new = '''        Some((observation, observed_files)) if observation.head_sha.is_none() => {
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
'''
if old not in text:
    raise SystemExit('missing wiki plan anchor')
text = text.replace(old, new, 1)

test_anchor = '''    #[test]
    fn matching_observation_is_idempotent() {
'''
test_insert = '''    #[test]
    fn uninitialized_provider_plans_non_mutating_bootstrap_requirement() {
        let root = test_project("bootstrap", "Home.md");
        fs::write(root.join(".project/wiki/Home.md"), "# Home\\n").unwrap();
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
'''
if test_anchor not in text:
    raise SystemExit('missing wiki test anchor')
wiki.write_text(text.replace(test_anchor, test_insert, 1))

# Treat the bootstrap requirement as an explicit non-mutating operation at apply time.
runtime = Path('crates/allodium-github/src/lib.rs')
text = runtime.read_text()
old = '''    pub wikis_observed: usize,
    pub wikis_updated: usize,
}
'''
new = '''    pub wikis_observed: usize,
    pub wikis_updated: usize,
    pub wiki_bootstrap_required: usize,
}
'''
if old not in text:
    raise SystemExit('missing apply report anchor')
text = text.replace(old, new, 1)

old = '''                "update_wiki" => {
                    wiki::apply_wiki_update(self, root, &plan.remote)?;
                    report.wikis_updated += 1;
                }
'''
new = '''                "wiki_bootstrap_required" => {
                    report.wiki_bootstrap_required += 1;
                }
                "update_wiki" => {
                    wiki::apply_wiki_update(self, root, &plan.remote)?;
                    report.wikis_updated += 1;
                }
'''
if old not in text:
    raise SystemExit('missing wiki apply anchor')
runtime.write_text(text.replace(old, new, 1))

# Merge release operations into the normal network-free GitHub plan and report bootstrap status.
cli = Path('crates/allodium-cli/src/main.rs')
text = cli.read_text()
old = '''        plan.operations.extend(wiki_plan.operations);
        Ok(plan)
'''
new = '''        plan.operations.extend(wiki_plan.operations);

        let release_plan = allodium_core::github_release::plan_releases(&root, remote_name)?;
        if plan.remote != release_plan.remote || plan.repository != release_plan.repository {
            return Err("GitHub Release projection plan disagrees about the configured remote".into());
        }
        plan.operations.extend(release_plan.operations);
        Ok(plan)
'''
if old not in text:
    raise SystemExit('missing CLI planner anchor')
text = text.replace(old, new, 1)
old = '''            println!("updated {} GitHub wiki(s)", report.wikis_updated);
            ExitCode::SUCCESS
'''
new = '''            println!("updated {} GitHub wiki(s)", report.wikis_updated);
            println!(
                "{} GitHub wiki projection(s) require provider bootstrap",
                report.wiki_bootstrap_required
            );
            ExitCode::SUCCESS
'''
if old not in text:
    raise SystemExit('missing CLI apply output anchor')
cli.write_text(text.replace(old, new, 1))
