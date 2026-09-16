from pathlib import Path

runtime = Path('crates/allodium-github/src/lib.rs')
text = runtime.read_text()

old = 'mod review_ingress;\nmod wiki;\n'
new = 'mod release;\nmod review_ingress;\nmod wiki;\n'
if old not in text:
    raise SystemExit('missing runtime module anchor')
text = text.replace(old, new, 1)

old = 'use allodium_core::{CanonicalIssue, CanonicalReview, load_issues, load_remote, load_reviews};\n'
new = 'use allodium_core::release::{CanonicalRelease, load_releases};\nuse allodium_core::{CanonicalIssue, CanonicalReview, load_issues, load_remote, load_reviews};\n'
if old not in text:
    raise SystemExit('missing runtime import anchor')
text = text.replace(old, new, 1)

old = '''    pub wiki_remote_changes_archived: usize,
    pub review_conversation_comment_snapshots_archived: usize,
'''
new = '''    pub wiki_remote_changes_archived: usize,
    pub releases_observed: usize,
    pub release_managed_changes_archived: usize,
    pub review_conversation_comment_snapshots_archived: usize,
'''
if old not in text:
    raise SystemExit('missing observe report anchor')
text = text.replace(old, new, 1)

old = '''    pub wiki_bootstrap_required: usize,
}
'''
new = '''    pub wiki_bootstrap_required: usize,
    pub releases_created: usize,
    pub releases_updated: usize,
    pub releases_observed: usize,
}
'''
if old not in text:
    raise SystemExit('missing apply report anchor')
text = text.replace(old, new, 1)

old = '''        for issue in self.fetch_repository_issues()? {
'''
new = '''        let release_report = release::observe_releases(self, root, remote_name)?;
        report.releases_observed += release_report.observed;
        report.release_managed_changes_archived += release_report.managed_changes_archived;

        for issue in self.fetch_repository_issues()? {
'''
if old not in text:
    raise SystemExit('missing observe integration anchor')
text = text.replace(old, new, 1)

old = '''        let reviews = load_reviews(root)?
            .into_iter()
            .map(|review| (review.record.id.clone(), review))
            .collect::<BTreeMap<_, _>>();
        let mut issue_mappings = load_mappings(root, &plan.remote)?;
'''
new = '''        let reviews = load_reviews(root)?
            .into_iter()
            .map(|review| (review.record.id.clone(), review))
            .collect::<BTreeMap<_, _>>();
        let releases = load_releases(root)?
            .into_iter()
            .map(|release| (release.record.id.clone(), release))
            .collect::<BTreeMap<_, _>>();
        let mut issue_mappings = load_mappings(root, &plan.remote)?;
'''
if old not in text:
    raise SystemExit('missing apply release map anchor')
text = text.replace(old, new, 1)

old = '''                "observe_wiki" => {
'''
new = '''                "create_release" => {
                    let canonical = require_release(&releases, &operation.canonical_id)?;
                    release::apply_create_release(self, root, &plan.remote, canonical)?;
                    report.releases_created += 1;
                }
                "observe_release" => {
                    let _canonical = require_release(&releases, &operation.canonical_id)?;
                    let release_id = require_number(operation)?;
                    release::apply_observe_release(
                        self,
                        root,
                        &plan.remote,
                        &operation.canonical_id,
                        release_id,
                    )?;
                    report.releases_observed += 1;
                }
                "update_release" => {
                    let canonical = require_release(&releases, &operation.canonical_id)?;
                    let release_id = require_number(operation)?;
                    release::apply_update_release(
                        self,
                        root,
                        &plan.remote,
                        canonical,
                        release_id,
                        &operation.fields,
                    )?;
                    report.releases_updated += 1;
                }
                "observe_wiki" => {
'''
if old not in text:
    raise SystemExit('missing release operation anchor')
text = text.replace(old, new, 1)

old = '''fn require_number(operation: &allodium_core::github::GitHubOperation) -> Result<u64, String> {
    operation.number.ok_or_else(|| {
        format!(
            "GitHub operation {:?} for {:?} is missing issue number",
            operation.action, operation.canonical_id
        )
    })
}
'''
new = '''fn require_release<'a>(
    releases: &'a BTreeMap<String, CanonicalRelease>,
    canonical_id: &str,
) -> Result<&'a CanonicalRelease, String> {
    releases
        .get(canonical_id)
        .ok_or_else(|| format!("plan references missing canonical release {canonical_id:?}"))
}

fn require_number(operation: &allodium_core::github::GitHubOperation) -> Result<u64, String> {
    operation.number.ok_or_else(|| {
        format!(
            "GitHub operation {:?} for {:?} is missing its remote numeric identifier",
            operation.action, operation.canonical_id
        )
    })
}
'''
if old not in text:
    raise SystemExit('missing require-number anchor')
runtime.write_text(text.replace(old, new, 1))

cli = Path('crates/allodium-cli/src/main.rs')
text = cli.read_text()
old = '''            println!("observed {} GitHub wiki(s)", report.wikis_observed);
            println!(
                "archived {} GitHub Wiki remote-tree change(s)",
                report.wiki_remote_changes_archived
            );
'''
new = '''            println!("observed {} GitHub wiki(s)", report.wikis_observed);
            println!("observed {} GitHub release(s)", report.releases_observed);
            println!(
                "archived {} GitHub Wiki remote-tree change(s)",
                report.wiki_remote_changes_archived
            );
            println!(
                "archived {} GitHub Release managed-field remote change(s)",
                report.release_managed_changes_archived
            );
'''
if old not in text:
    raise SystemExit('missing CLI observe output anchor')
text = text.replace(old, new, 1)
old = '''            println!(
                "{} GitHub wiki projection(s) require provider bootstrap",
                report.wiki_bootstrap_required
            );
            ExitCode::SUCCESS
'''
new = '''            println!(
                "{} GitHub wiki projection(s) require provider bootstrap",
                report.wiki_bootstrap_required
            );
            println!("created {} GitHub release(s)", report.releases_created);
            println!("updated {} GitHub release(s)", report.releases_updated);
            println!("observed {} GitHub release(s)", report.releases_observed);
            ExitCode::SUCCESS
'''
if old not in text:
    raise SystemExit('missing CLI apply output anchor')
cli.write_text(text.replace(old, new, 1))

# Add an HTTP-level stale-write test to the release runtime module.
release_path = Path('crates/allodium-github/src/release.rs')
text = release_path.read_text()
if not text.endswith('\n}\n'):
    raise SystemExit('unexpected release module ending')
insert = r'''

    #[test]
    fn stale_release_update_sends_no_patch() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::{SystemTime, UNIX_EPOCH};

        const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&requests);
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 8192];
                let count = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let line = request.lines().next().unwrap_or_default().to_string();
                seen.lock().unwrap().push(line.clone());
                let body = if line.contains("/releases/42 ") {
                    format!(
                        r#"{{"id":42,"html_url":"https://example.invalid/release","tag_name":"v0.1.0","name":"Externally changed","body":"notes","draft":true,"prerelease":false,"published_at":null}}"#
                    )
                } else {
                    format!(r#"[{{"name":"v0.1.0","commit":{{"sha":"{SHA}"}}}}]"#)
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        });

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-release-stale-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let previous = ObservedRelease {
            schema: OBSERVED_RELEASE_SCHEMA_V0.into(),
            canonical_id: "release-0001".into(),
            id: 42,
            url: "https://example.invalid/release".into(),
            name: "Previously observed".into(),
            tag_name: "v0.1.0".into(),
            commit_sha: SHA.into(),
            draft: true,
            prerelease: false,
            published_at: None,
            observed_at: "2026-09-16T00:00:00Z".into(),
        };
        write_observed_release_snapshot(&root, "github", &previous, "notes").unwrap();
        let canonical = CanonicalRelease {
            record: allodium_core::release::ReleaseRecord {
                schema: allodium_core::release::RELEASE_SCHEMA_V0.into(),
                id: "release-0001".into(),
                title: "Canonical title".into(),
                version: "0.1.0".into(),
                state: "draft".into(),
                revision: SHA.into(),
                tag: Some("v0.1.0".into()),
            },
            notes: "notes".into(),
            directory: std::path::PathBuf::new(),
        };
        let adapter = GitHubAdapter {
            client: reqwest::blocking::Client::builder().build().unwrap(),
            repository: "owner/repo".into(),
            token: Some("test-token".into()),
            api_base: format!("http://{address}"),
        };

        let error = apply_update_release(
            &adapter,
            &root,
            "github",
            &canonical,
            42,
            &["title".into()],
        )
        .unwrap_err();
        assert!(error.contains("refusing stale update"), "{error}");
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| request.starts_with("GET ")));
        std::fs::remove_dir_all(root).unwrap();
    }
'''
release_path.write_text(text[:-3] + insert + '\n}\n')
