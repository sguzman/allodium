use allodium_core::github::{ArchiveOutcome, IncomingIssueComment, archive_issue_comment};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn richer_reobservation_does_not_rewrite_creation_evidence() {
    let root = test_root();
    write_remote(&root);

    let first = IncomingIssueComment {
        canonical_id: "issue-0002".into(),
        issue_number: 2,
        comment_id: 5_697_573_876,
        actor_remote_id: "6679733".into(),
        actor_login: "sguzman".into(),
        url: "https://github.com/sguzman/allodium/issues/2#issuecomment-5697573876".into(),
        body: "same body".into(),
        observed_at: "2026-09-16T12:40:34Z".into(),
        created_at: None,
        updated_at: None,
        evidence: "sparse connector capture".into(),
    };

    let mut richer = first.clone();
    richer.observed_at = "2026-09-16T13:10:00Z".into();
    richer.created_at = Some("2026-09-16T12:40:22Z".into());
    richer.updated_at = Some("2026-09-16T12:40:22Z".into());
    richer.evidence = "GitHub REST issue-comment observation".into();

    assert!(matches!(
        archive_issue_comment(&root, "github", &first).unwrap(),
        ArchiveOutcome::Created(_)
    ));
    assert!(matches!(
        archive_issue_comment(&root, "github", &richer).unwrap(),
        ArchiveOutcome::Existing(_)
    ));

    let event = fs::read_to_string(root.join(
        ".project/remotes/github/incoming/2026/09/github-issue-comment-5697573876-created/event.toml",
    ))
    .unwrap();
    assert!(event.contains("sparse connector capture"));
    assert!(!event.contains("GitHub REST issue-comment observation"));

    fs::remove_dir_all(root).unwrap();
}

fn write_remote(root: &Path) {
    fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
    fs::write(
        root.join(".project/remotes/github/remote.toml"),
        "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"sguzman/allodium\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
    )
    .unwrap();
}

fn test_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "allodium-ingress-idempotence-{}-{nonce}",
        std::process::id()
    ))
}
