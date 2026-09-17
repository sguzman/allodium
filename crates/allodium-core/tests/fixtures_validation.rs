use allodium_core::validate;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn checked_in_valid_fixture_is_accepted() {
    let report = validate(fixture("valid-minimal"));
    assert!(report.is_ok(), "{:?}", report.errors);
}

#[test]
fn checked_in_issue_identity_mismatch_is_rejected() {
    let report = validate(fixture("invalid-issue-id"));
    assert!(!report.is_ok());
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("must match directory"))
    );
}

#[test]
fn checked_in_invalid_issue_state_is_rejected() {
    let report = validate(fixture("invalid-issue-state"));
    assert!(!report.is_ok());
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("state must be open or closed"))
    );
}

#[test]
fn checked_in_unknown_project_schema_is_rejected() {
    let report = validate(fixture("invalid-project-schema"));
    assert!(!report.is_ok());
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("unsupported schema"))
    );
}

#[test]
fn checked_in_valid_review_fixture_is_accepted() {
    let report = validate(fixture("valid-review"));
    assert!(report.is_ok(), "{:?}", report.errors);
}

#[test]
fn checked_in_review_with_same_base_and_head_is_rejected() {
    let report = validate(fixture("invalid-review-same-ref"));
    assert!(!report.is_ok());
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("base and head must differ"))
    );
}

#[test]
fn checked_in_valid_board_fixture_is_accepted() {
    let report = validate(fixture("valid-board"));
    assert!(report.is_ok(), "{:?}", report.errors);
}

#[test]
fn checked_in_board_with_invalid_select_value_is_rejected() {
    let report = validate(fixture("invalid-board-value"));
    assert!(!report.is_ok());
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("requires a declared option id"))
    );
}
