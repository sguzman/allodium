from pathlib import Path

core = Path('crates/allodium-core/src/github.rs')
runtime = Path('crates/allodium-github/src/lib.rs')


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'{label}: expected exactly one anchor, found {count}')
    return text.replace(old, new, 1)

core_text = core.read_text()
core_text = replace_once(
    core_text,
    '    pub head_sha: String,\n    pub remote_updated_at: String,\n',
    '    pub head_sha: String,\n    #[serde(default, skip_serializing_if = "Option::is_none")]\n    pub merged_at: Option<String>,\n    pub remote_updated_at: String,\n',
    'ObservedReview merged_at field',
)
core_text = replace_once(
    core_text,
    '''    #[test]\n    fn canonical_merge_is_not_an_implicit_github_merge_command() {\n        let root = test_project("review-merge");\n        write_review(&root, "merged", "main", "feature");\n        write_review_mapping(&root);\n        write_matching_review_observation(&root, "open", "main", "feature");\n\n        let error = plan_reviews(&root, "github").unwrap_err();\n        assert!(error.contains("merge as observed remote evidence"));\n\n        fs::remove_dir_all(root).unwrap();\n    }\n''',
    '''    #[test]\n    fn canonical_merge_is_not_an_implicit_github_merge_command() {\n        let root = test_project("review-merge");\n        write_review(&root, "merged", "main", "feature");\n        write_review_mapping(&root);\n        write_matching_review_observation(&root, "open", "main", "feature");\n\n        let error = plan_reviews(&root, "github").unwrap_err();\n        assert!(error.contains("merge as observed remote evidence"));\n\n        fs::remove_dir_all(root).unwrap();\n    }\n\n    #[test]\n    fn observed_review_merge_timestamp_is_optional_and_round_trips() {\n        let legacy = r#"\nschema = "allodium.github.observed-review/v0"\ncanonical_id = "review-0001"\nnumber = 23\nstate = "closed"\ntitle = "Test review"\nurl = "https://github.com/owner/repo/pull/23"\nbase_ref = "main"\nhead_ref = "feature"\nbase_sha = "base-sha"\nhead_sha = "head-sha"\nremote_updated_at = "2026-09-16T12:00:00Z"\nobserved_at = "2026-09-16T12:01:00Z"\n"#;\n        let legacy: ObservedReview = toml::from_str(legacy).unwrap();\n        assert_eq!(legacy.merged_at, None);\n\n        let merged = ObservedReview {\n            state: "merged".into(),\n            merged_at: Some("2026-09-16T12:02:00Z".into()),\n            ..legacy\n        };\n        let encoded = toml::to_string_pretty(&merged).unwrap();\n        assert!(encoded.contains("state = \\"merged\\""));\n        assert!(encoded.contains("merged_at = \\"2026-09-16T12:02:00Z\\""));\n        let decoded: ObservedReview = toml::from_str(&encoded).unwrap();\n        assert_eq!(decoded, merged);\n    }\n''',
    'merge provenance compatibility test',
)
core.write_text(core_text)

runtime_text = runtime.read_text()
needle = '''        head_sha: review.head.sha.clone(),\n        remote_updated_at: review.updated_at.clone(),\n'''
replacement = '''        head_sha: review.head.sha.clone(),\n        merged_at: review.merged_at.clone(),\n        remote_updated_at: review.updated_at.clone(),\n'''
count = runtime_text.count(needle)
if count != 1:
    raise SystemExit(f'write_observed_review constructor: expected one anchor, found {count}')
runtime_text = runtime_text.replace(needle, replacement, 1)

needle = '''        head_sha: live.head.sha.clone(),\n        remote_updated_at: live.updated_at.clone(),\n'''
replacement = '''        head_sha: live.head.sha.clone(),\n        merged_at: live.merged_at.clone(),\n        remote_updated_at: live.updated_at.clone(),\n'''
count = runtime_text.count(needle)
if count != 1:
    raise SystemExit(f'archive after constructor: expected one anchor, found {count}')
runtime_text = runtime_text.replace(needle, replacement, 1)
runtime.write_text(runtime_text)
