use crate::{ValidationReport, load_issues, load_reviews};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const BOARD_SCHEMA_V0: &str = "allodium.board/v0";
pub const BOARD_FIELD_SCHEMA_V0: &str = "allodium.board-field/v0";
pub const BOARD_ITEM_SCHEMA_V0: &str = "allodium.board-item/v0";
pub const BOARD_VIEW_SCHEMA_V0: &str = "allodium.board-view/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoardRecord {
    pub schema: String,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoardFieldOption {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoardFieldRecord {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub options: Vec<BoardFieldOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoardItemRecord {
    pub schema: String,
    pub object: String,
    #[serde(default)]
    pub values: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoardViewRecord {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub layout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_field: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBoardField {
    pub record: BoardFieldRecord,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalBoardItem {
    pub record: BoardItemRecord,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBoardView {
    pub record: BoardViewRecord,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalBoard {
    pub record: BoardRecord,
    pub directory: PathBuf,
    pub fields: Vec<CanonicalBoardField>,
    pub items: Vec<CanonicalBoardItem>,
    pub views: Vec<CanonicalBoardView>,
}

pub fn load_boards(root: impl AsRef<Path>) -> Result<Vec<CanonicalBoard>, String> {
    let boards_dir = root.as_ref().join(".project/boards");
    if !boards_dir.exists() {
        return Ok(Vec::new());
    }
    if !boards_dir.is_dir() {
        return Err(format!("{}: expected a directory", boards_dir.display()));
    }

    let mut entries = fs::read_dir(&boards_dir)
        .map_err(|error| format!("{}: {error}", boards_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", boards_dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut boards = Vec::new();
    for entry in entries {
        let directory = entry.path();
        if !directory.is_dir() {
            continue;
        }

        let record_path = directory.join("board.toml");
        let record: BoardRecord = read_toml(&record_path)?;
        let fields = load_fields(&directory.join("fields"))?;
        let items = load_items(&directory.join("items"))?;
        let views = load_views(&directory.join("views"))?;

        boards.push(CanonicalBoard {
            record,
            directory,
            fields,
            items,
            views,
        });
    }

    boards.sort_by(|left, right| left.record.id.cmp(&right.record.id));
    Ok(boards)
}

pub(crate) fn validate_boards(root: &Path, report: &mut ValidationReport) {
    let boards = match load_boards(root) {
        Ok(boards) => boards,
        Err(error) => {
            report.errors.push(error);
            return;
        }
    };

    let mut canonical_objects = BTreeSet::new();
    match load_issues(root) {
        Ok(issues) => {
            canonical_objects.extend(issues.into_iter().map(|issue| issue.record.id));
        }
        Err(error) => report.errors.push(error),
    }
    match load_reviews(root) {
        Ok(reviews) => {
            canonical_objects.extend(reviews.into_iter().map(|review| review.record.id));
        }
        Err(error) => report.errors.push(error),
    }

    let mut board_ids = BTreeSet::new();
    for board in boards {
        validate_board(&board, &canonical_objects, report);
        if !board_ids.insert(board.record.id.clone()) {
            report.errors.push(format!(
                "{}: duplicate board id {:?}",
                board.directory.display(),
                board.record.id
            ));
        }
    }
}

fn validate_board(
    board: &CanonicalBoard,
    canonical_objects: &BTreeSet<String>,
    report: &mut ValidationReport,
) {
    let record_path = board.directory.join("board.toml");
    let expected_id = board
        .directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if board.record.schema != BOARD_SCHEMA_V0 {
        report.errors.push(format!(
            "{}: unsupported schema {:?}; expected {:?}",
            record_path.display(),
            board.record.schema,
            BOARD_SCHEMA_V0
        ));
    }
    if board.record.id != expected_id {
        report.errors.push(format!(
            "{}: id {:?} must match directory {:?}",
            record_path.display(),
            board.record.id,
            expected_id
        ));
    }
    if board.record.title.trim().is_empty() {
        report.errors.push(format!(
            "{}: title must not be empty",
            record_path.display()
        ));
    }

    let mut fields = BTreeMap::new();
    for field in &board.fields {
        validate_field(field, report);
        if fields.insert(field.record.id.clone(), field).is_some() {
            report.errors.push(format!(
                "{}: duplicate field id {:?}",
                field.path.display(),
                field.record.id
            ));
        }
    }

    let mut objects = BTreeSet::new();
    for item in &board.items {
        validate_item(item, &fields, canonical_objects, report);
        if !objects.insert(item.record.object.clone()) {
            report.errors.push(format!(
                "{}: duplicate board item for canonical object {:?}",
                item.path.display(),
                item.record.object
            ));
        }
    }

    let mut views = BTreeSet::new();
    for view in &board.views {
        validate_view(view, &fields, report);
        if !views.insert(view.record.id.clone()) {
            report.errors.push(format!(
                "{}: duplicate view id {:?}",
                view.path.display(),
                view.record.id
            ));
        }
    }
}

fn validate_field(field: &CanonicalBoardField, report: &mut ValidationReport) {
    let expected_id = field
        .path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if field.record.schema != BOARD_FIELD_SCHEMA_V0 {
        report.errors.push(format!(
            "{}: unsupported schema {:?}; expected {:?}",
            field.path.display(),
            field.record.schema,
            BOARD_FIELD_SCHEMA_V0
        ));
    }
    if field.record.id != expected_id {
        report.errors.push(format!(
            "{}: id {:?} must match filename stem {:?}",
            field.path.display(),
            field.record.id,
            expected_id
        ));
    }
    if field.record.name.trim().is_empty() {
        report
            .errors
            .push(format!("{}: name must not be empty", field.path.display()));
    }

    if !matches!(
        field.record.kind.as_str(),
        "text" | "number" | "date" | "single_select"
    ) {
        report.errors.push(format!(
            "{}: kind must be text, number, date, or single_select",
            field.path.display()
        ));
    }

    if field.record.kind == "single_select" {
        if field.record.options.is_empty() {
            report.errors.push(format!(
                "{}: single_select fields require at least one option",
                field.path.display()
            ));
        }
        let mut option_ids = BTreeSet::new();
        for option in &field.record.options {
            if option.id.trim().is_empty() || option.name.trim().is_empty() {
                report.errors.push(format!(
                    "{}: single_select option id and name must not be empty",
                    field.path.display()
                ));
            }
            if !option_ids.insert(option.id.clone()) {
                report.errors.push(format!(
                    "{}: duplicate single_select option id {:?}",
                    field.path.display(),
                    option.id
                ));
            }
        }
    } else if !field.record.options.is_empty() {
        report.errors.push(format!(
            "{}: only single_select fields may declare options",
            field.path.display()
        ));
    }
}

fn validate_item(
    item: &CanonicalBoardItem,
    fields: &BTreeMap<String, &CanonicalBoardField>,
    canonical_objects: &BTreeSet<String>,
    report: &mut ValidationReport,
) {
    let expected_object = item
        .path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if item.record.schema != BOARD_ITEM_SCHEMA_V0 {
        report.errors.push(format!(
            "{}: unsupported schema {:?}; expected {:?}",
            item.path.display(),
            item.record.schema,
            BOARD_ITEM_SCHEMA_V0
        ));
    }
    if item.record.object != expected_object {
        report.errors.push(format!(
            "{}: object {:?} must match filename stem {:?}",
            item.path.display(),
            item.record.object,
            expected_object
        ));
    }
    if !canonical_objects.contains(&item.record.object) {
        report.errors.push(format!(
            "{}: object {:?} does not reference an existing canonical issue or review",
            item.path.display(),
            item.record.object
        ));
    }

    for (field_id, value) in &item.record.values {
        let Some(field) = fields.get(field_id) else {
            report.errors.push(format!(
                "{}: value references unknown field {:?}",
                item.path.display(),
                field_id
            ));
            continue;
        };
        validate_value(item, field, value, report);
    }
}

fn validate_value(
    item: &CanonicalBoardItem,
    field: &CanonicalBoardField,
    value: &toml::Value,
    report: &mut ValidationReport,
) {
    let valid = match field.record.kind.as_str() {
        "text" => value.as_str().is_some(),
        "number" => value.as_integer().is_some() || value.as_float().is_some(),
        "date" => value.as_str().is_some_and(valid_calendar_date),
        "single_select" => value.as_str().is_some_and(|selected| {
            field
                .record
                .options
                .iter()
                .any(|option| option.id == selected)
        }),
        _ => true,
    };

    if valid {
        return;
    }

    let expectation = match field.record.kind.as_str() {
        "text" => "requires a string value",
        "number" => "requires an integer or float value",
        "date" => "requires an ISO 8601 calendar date (YYYY-MM-DD)",
        "single_select" => "requires a declared option id",
        _ => "has an unsupported field kind",
    };
    report.errors.push(format!(
        "{}: field {:?} {expectation}",
        item.path.display(),
        field.record.id
    ));
}

fn validate_view(
    view: &CanonicalBoardView,
    fields: &BTreeMap<String, &CanonicalBoardField>,
    report: &mut ValidationReport,
) {
    let expected_id = view
        .path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if view.record.schema != BOARD_VIEW_SCHEMA_V0 {
        report.errors.push(format!(
            "{}: unsupported schema {:?}; expected {:?}",
            view.path.display(),
            view.record.schema,
            BOARD_VIEW_SCHEMA_V0
        ));
    }
    if view.record.id != expected_id {
        report.errors.push(format!(
            "{}: id {:?} must match filename stem {:?}",
            view.path.display(),
            view.record.id,
            expected_id
        ));
    }
    if view.record.name.trim().is_empty() {
        report
            .errors
            .push(format!("{}: name must not be empty", view.path.display()));
    }
    if !matches!(view.record.layout.as_str(), "table" | "board" | "roadmap") {
        report.errors.push(format!(
            "{}: layout must be table, board, or roadmap",
            view.path.display()
        ));
    }

    if view.record.layout == "board" && view.record.group_by.is_none() {
        report.errors.push(format!(
            "{}: board views require group_by",
            view.path.display()
        ));
    }
    if view.record.layout == "roadmap"
        && view.record.start_field.is_none()
        && view.record.end_field.is_none()
    {
        report.errors.push(format!(
            "{}: roadmap views require start_field or end_field",
            view.path.display()
        ));
    }

    validate_view_field_ref(
        view,
        "group_by",
        view.record.group_by.as_deref(),
        fields,
        false,
        report,
    );
    validate_view_field_ref(
        view,
        "start_field",
        view.record.start_field.as_deref(),
        fields,
        true,
        report,
    );
    validate_view_field_ref(
        view,
        "end_field",
        view.record.end_field.as_deref(),
        fields,
        true,
        report,
    );
}

fn validate_view_field_ref(
    view: &CanonicalBoardView,
    property: &str,
    field_id: Option<&str>,
    fields: &BTreeMap<String, &CanonicalBoardField>,
    require_date: bool,
    report: &mut ValidationReport,
) {
    let Some(field_id) = field_id else {
        return;
    };
    let Some(field) = fields.get(field_id) else {
        report.errors.push(format!(
            "{}: {property} references unknown field {:?}",
            view.path.display(),
            field_id
        ));
        return;
    };
    if require_date && field.record.kind != "date" {
        report.errors.push(format!(
            "{}: {property} field {:?} must have kind date",
            view.path.display(),
            field_id
        ));
    }
}

fn load_fields(path: &Path) -> Result<Vec<CanonicalBoardField>, String> {
    load_toml_files(path, |record, path| CanonicalBoardField { record, path })
}

fn load_items(path: &Path) -> Result<Vec<CanonicalBoardItem>, String> {
    load_toml_files(path, |record, path| CanonicalBoardItem { record, path })
}

fn load_views(path: &Path) -> Result<Vec<CanonicalBoardView>, String> {
    load_toml_files(path, |record, path| CanonicalBoardView { record, path })
}

fn load_toml_files<T, U>(path: &Path, make: impl Fn(T, PathBuf) -> U) -> Result<Vec<U>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    if !path.is_dir() {
        return Err(format!("{}: expected a directory", path.display()));
    }

    let mut entries = fs::read_dir(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut records = Vec::new();
    for entry in entries {
        let record_path = entry.path();
        if !record_path.is_file()
            || record_path.extension().and_then(|value| value.to_str()) != Some("toml")
        {
            continue;
        }
        let record = read_toml(&record_path)?;
        records.push(make(record, record_path));
    }
    Ok(records)
}

fn read_toml<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

fn valid_calendar_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }

    let year = value[0..4].parse::<u32>().ok();
    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    if !(1..=12).contains(&month) || day == 0 {
        return false;
    }

    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day <= max_day
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validator_accepts_filesystem_native_board() {
        let root = test_root("valid");
        write_board(&root, "active", true);

        let report = crate::validate(&root);
        assert!(report.is_ok(), "{:?}", report.errors);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_dangling_object_and_unknown_select_option() {
        let root = test_root("bad-item");
        write_board(&root, "not-an-option", true);
        let item_path = root.join(".project/boards/board-0001/items/issue-0001.toml");
        let item = fs::read_to_string(&item_path).unwrap();
        fs::write(
            root.join(".project/boards/board-0001/items/issue-9999.toml"),
            item.replace("object = \"issue-0001\"", "object = \"issue-9999\""),
        )
        .unwrap();

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("requires a declared option id"))
        );
        assert!(report.errors.iter().any(|error| {
            error.contains("does not reference an existing canonical issue or review")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_invalid_view_semantics() {
        let root = test_root("bad-view");
        write_board(&root, "active", false);
        fs::write(
            root.join(".project/boards/board-0001/views/roadmap.toml"),
            "schema = \"allodium.board-view/v0\"\nid = \"roadmap\"\nname = \"Roadmap\"\nlayout = \"roadmap\"\nend_field = \"status\"\n",
        )
        .unwrap();

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("board views require group_by"))
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("must have kind date"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn write_board(root: &Path, status: &str, group_board: bool) {
        let board = root.join(".project/boards/board-0001");
        fs::create_dir_all(board.join("fields")).unwrap();
        fs::create_dir_all(board.join("items")).unwrap();
        fs::create_dir_all(board.join("views")).unwrap();
        fs::write(
            board.join("board.toml"),
            "schema = \"allodium.board/v0\"\nid = \"board-0001\"\ntitle = \"Development\"\ndescription = \"Test board\"\n",
        )
        .unwrap();
        fs::write(
            board.join("fields/status.toml"),
            "schema = \"allodium.board-field/v0\"\nid = \"status\"\nname = \"Status\"\nkind = \"single_select\"\n\n[[options]]\nid = \"active\"\nname = \"Active\"\n\n[[options]]\nid = \"done\"\nname = \"Done\"\n",
        )
        .unwrap();
        fs::write(
            board.join("fields/target-date.toml"),
            "schema = \"allodium.board-field/v0\"\nid = \"target-date\"\nname = \"Target date\"\nkind = \"date\"\n",
        )
        .unwrap();
        fs::write(
            board.join("items/issue-0001.toml"),
            format!(
                "schema = \"allodium.board-item/v0\"\nobject = \"issue-0001\"\n\n[values]\nstatus = \"{status}\"\ntarget-date = \"2026-09-30\"\n"
            ),
        )
        .unwrap();
        fs::write(
            board.join("views/development.toml"),
            if group_board {
                "schema = \"allodium.board-view/v0\"\nid = \"development\"\nname = \"Development\"\nlayout = \"board\"\ngroup_by = \"status\"\n"
            } else {
                "schema = \"allodium.board-view/v0\"\nid = \"development\"\nname = \"Development\"\nlayout = \"board\"\n"
            },
        )
        .unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-board-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".project/issues/issue-0001")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::write(
            root.join(".project/manifest.toml"),
            "schema = \"allodium.project/v0\"\nid = \"test\"\nname = \"Test\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/issues/issue-0001/issue.toml"),
            "schema = \"allodium.issue/v0\"\nid = \"issue-0001\"\ntitle = \"Test\"\nstate = \"open\"\n",
        )
        .unwrap();
        fs::write(root.join(".project/issues/issue-0001/body.md"), "Body\n").unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        root
    }
}
