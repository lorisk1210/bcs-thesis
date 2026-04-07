use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime, Utc};
use duckdb::types::ValueRef;
use duckdb::{AccessMode, Config, Connection};
use walkdir::WalkDir;

use crate::error::{ViewerError, ViewerResult};
use crate::models::{
    ColumnInfo, DatabaseKind, DatabaseListEntry, DatabaseListPage, DatabaseOverview, RelationKind,
    RelationSummary, SummaryCard, TableCategory, TablePage,
};

const DEFAULT_PAGE_SIZE: usize = 50;

pub fn list_databases(data_dir: &Path) -> ViewerResult<DatabaseListPage> {
    if !data_dir.exists() {
        return Ok(DatabaseListPage {
            data_dir_display: display_path(data_dir),
            databases: Vec::new(),
        });
    }

    let canonical_root = canonical_data_dir(data_dir)?;
    let mut databases = Vec::new();

    for entry in WalkDir::new(&canonical_root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|ext| ext.to_str()) != Some("duckdb")
        {
            continue;
        }

        let absolute_path = entry
            .path()
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", entry.path().display()))
            .map_err(|error| ViewerError::internal(error.to_string()))?;
        let relative_path = strip_to_relative(&canonical_root, &absolute_path)?;
        let metadata = read_file_metadata(&absolute_path)?;

        let inspection = inspect_database_file(&absolute_path);
        let (kind, relation_count, inspection_error) = match inspection {
            Ok((kind, relation_count)) => (kind, Some(relation_count), None),
            Err(error) => (
                DatabaseKind::Unknown,
                None,
                Some(error.message().to_string()),
            ),
        };

        databases.push(DatabaseListEntry {
            file_name: absolute_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown.duckdb")
                .to_string(),
            relative_path,
            size_bytes: metadata.size_bytes,
            modified_at: metadata.modified_at,
            kind,
            relation_count,
            inspection_error,
        });
    }

    databases.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(DatabaseListPage {
        data_dir_display: display_path(&canonical_root),
        databases,
    })
}

pub fn load_database_overview(
    data_dir: &Path,
    requested_db: &str,
) -> ViewerResult<DatabaseOverview> {
    let resolved = resolve_database_path(data_dir, requested_db)?;
    let connection = open_read_only(&resolved.absolute_path)?;
    let mut relations = load_relations(&connection)?;
    relations.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then(left.name.cmp(&right.name))
    });

    let kind = detect_database_kind(relations.iter().map(|relation| relation.name.as_str()));
    let cards = build_summary_cards(kind, &relations);

    Ok(DatabaseOverview {
        relative_path: resolved.relative_path,
        file_name: resolved.file_name,
        size_bytes: resolved.size_bytes,
        modified_at: resolved.modified_at,
        kind,
        cards,
        relations,
    })
}

pub fn load_table_page(
    data_dir: &Path,
    requested_db: &str,
    relation_name: &str,
    page: usize,
) -> ViewerResult<TablePage> {
    if relation_name.trim().is_empty() {
        return Err(ViewerError::bad_request("relation name must not be empty"));
    }
    if page == 0 {
        return Err(ViewerError::bad_request("page must be at least 1"));
    }

    let resolved = resolve_database_path(data_dir, requested_db)?;
    let connection = open_read_only(&resolved.absolute_path)?;
    let relations = load_relations(&connection)?;
    let database_kind =
        detect_database_kind(relations.iter().map(|relation| relation.name.as_str()));
    let relation = relations
        .iter()
        .find(|candidate| candidate.name == relation_name)
        .cloned()
        .ok_or_else(|| {
            ViewerError::not_found(format!("relation '{relation_name}' was not found"))
        })?;

    let columns = load_columns(&connection, relation_name)?;
    let offset = (page - 1) * DEFAULT_PAGE_SIZE;
    let rows = load_rows(&connection, relation_name, DEFAULT_PAGE_SIZE, offset)?;
    if relation.row_count > 0 && rows.is_empty() && offset >= relation.row_count as usize {
        return Err(ViewerError::bad_request("requested page is out of range"));
    }

    Ok(TablePage {
        database_relative_path: resolved.relative_path,
        database_file_name: resolved.file_name,
        database_kind,
        relation_name: relation.name,
        relation_kind: relation.relation_kind,
        category: relation.category,
        columns,
        rows,
        page,
        page_size: DEFAULT_PAGE_SIZE,
        total_rows: relation.row_count,
        has_previous_page: page > 1,
        has_next_page: offset + DEFAULT_PAGE_SIZE < relation.row_count as usize,
    })
}

fn inspect_database_file(path: &Path) -> ViewerResult<(DatabaseKind, usize)> {
    let connection = open_read_only(path)?;
    let relations = load_relations(&connection)?;
    let kind = detect_database_kind(relations.iter().map(|relation| relation.name.as_str()));
    Ok((kind, relations.len()))
}

fn open_read_only(path: &Path) -> ViewerResult<Connection> {
    let config = Config::default()
        .access_mode(AccessMode::ReadOnly)
        .with_context(|| {
            format!(
                "failed to configure read-only access for {}",
                path.display()
            )
        })
        .map_err(|error| ViewerError::internal(error.to_string()))?
        .enable_external_access(false)
        .with_context(|| format!("failed to disable external access for {}", path.display()))
        .map_err(|error| ViewerError::internal(error.to_string()))?;

    Connection::open_with_flags(path, config)
        .with_context(|| format!("failed to open {} in read-only mode", path.display()))
        .map_err(|error| ViewerError::internal(error.to_string()))
}

fn load_relations(connection: &Connection) -> ViewerResult<Vec<RelationSummary>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT table_name, table_type
            FROM information_schema.tables
            WHERE table_schema = 'main'
            ORDER BY table_name
            "#,
        )
        .context("failed to load relation list")
        .map_err(|error| ViewerError::internal(error.to_string()))?;

    let relation_pairs = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("failed to query relation list")
        .map_err(|error| ViewerError::internal(error.to_string()))?
        .collect::<duckdb::Result<Vec<_>>>()
        .context("failed to collect relation list")
        .map_err(|error| ViewerError::internal(error.to_string()))?;

    let mut relations = Vec::with_capacity(relation_pairs.len());
    for (name, table_type) in relation_pairs {
        let relation_kind = if table_type.eq_ignore_ascii_case("VIEW") {
            RelationKind::View
        } else {
            RelationKind::Table
        };
        let row_count = relation_row_count(connection, &name)?;
        let column_count = relation_column_count(connection, &name)?;
        relations.push(RelationSummary {
            category: categorize_relation(&name),
            name,
            relation_kind,
            row_count,
            column_count,
        });
    }

    Ok(relations)
}

fn load_columns(connection: &Connection, relation_name: &str) -> ViewerResult<Vec<ColumnInfo>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_schema = 'main' AND table_name = ?1
            ORDER BY ordinal_position
            "#,
        )
        .with_context(|| format!("failed to prepare schema query for {relation_name}"))
        .map_err(|error| ViewerError::internal(error.to_string()))?;

    statement
        .query_map([relation_name], |row| {
            let nullable: String = row.get(2)?;
            Ok(ColumnInfo {
                name: row.get(0)?,
                data_type: row.get(1)?,
                nullable: nullable.eq_ignore_ascii_case("YES"),
            })
        })
        .with_context(|| format!("failed to query schema for {relation_name}"))
        .map_err(|error| ViewerError::internal(error.to_string()))?
        .collect::<duckdb::Result<Vec<_>>>()
        .with_context(|| format!("failed to collect schema for {relation_name}"))
        .map_err(|error| ViewerError::internal(error.to_string()))
}

fn load_rows(
    connection: &Connection,
    relation_name: &str,
    limit: usize,
    offset: usize,
) -> ViewerResult<Vec<Vec<String>>> {
    let query = format!(
        "SELECT * FROM {} LIMIT ?1 OFFSET ?2",
        quote_identifier(relation_name)
    );
    let mut statement = connection
        .prepare(&query)
        .with_context(|| format!("failed to prepare row query for {relation_name}"))
        .map_err(|error| ViewerError::internal(error.to_string()))?;
    let mut rows = statement
        .query([limit as i64, offset as i64])
        .with_context(|| format!("failed to fetch rows for {relation_name}"))
        .map_err(|error| ViewerError::internal(error.to_string()))?;

    let column_count = rows
        .as_ref()
        .map(|statement| statement.schema().fields().len())
        .unwrap_or_default();

    let mut rendered_rows = Vec::new();
    while let Some(row) = rows
        .next()
        .with_context(|| format!("failed while streaming rows for {relation_name}"))
        .map_err(|error| ViewerError::internal(error.to_string()))?
    {
        let mut rendered = Vec::with_capacity(column_count);
        for index in 0..column_count {
            let value = row
                .get_ref(index)
                .with_context(|| format!("failed to read column {index} from {relation_name}"))
                .map_err(|error| ViewerError::internal(error.to_string()))?;
            rendered.push(render_value(value));
        }
        rendered_rows.push(rendered);
    }

    Ok(rendered_rows)
}

fn relation_row_count(connection: &Connection, relation_name: &str) -> ViewerResult<u64> {
    let query = format!("SELECT COUNT(*) FROM {}", quote_identifier(relation_name));
    let count: i64 = connection
        .query_row(&query, [], |row| row.get(0))
        .with_context(|| format!("failed to count rows for {relation_name}"))
        .map_err(|error| ViewerError::internal(error.to_string()))?;
    Ok(count.max(0) as u64)
}

fn relation_column_count(connection: &Connection, relation_name: &str) -> ViewerResult<usize> {
    let count: i64 = connection
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM information_schema.columns
            WHERE table_schema = 'main' AND table_name = ?1
            "#,
            [relation_name],
            |row| row.get(0),
        )
        .with_context(|| format!("failed to count columns for {relation_name}"))
        .map_err(|error| ViewerError::internal(error.to_string()))?;
    Ok(count.max(0) as usize)
}

fn build_summary_cards(kind: DatabaseKind, relations: &[RelationSummary]) -> Vec<SummaryCard> {
    let counts: HashMap<&str, u64> = relations
        .iter()
        .map(|relation| (relation.name.as_str(), relation.row_count))
        .collect();

    let mut cards = vec![
        SummaryCard {
            label: "Relations".to_string(),
            value: relations.len().to_string(),
        },
        SummaryCard {
            label: "Rows across relations".to_string(),
            value: relations
                .iter()
                .map(|relation| relation.row_count)
                .sum::<u64>()
                .to_string(),
        },
    ];

    match kind {
        DatabaseKind::RefineryNode => {
            cards.extend([
                SummaryCard {
                    label: "Patients".to_string(),
                    value: pick_count(&counts, &["patient_dim", "bronze_patient"]),
                },
                SummaryCard {
                    label: "Conditions".to_string(),
                    value: pick_count(&counts, &["condition_fact", "bronze_condition"]),
                },
                SummaryCard {
                    label: "Medications".to_string(),
                    value: pick_count(&counts, &["medication_fact", "bronze_medication_request"]),
                },
                SummaryCard {
                    label: "Observations".to_string(),
                    value: pick_count(&counts, &["observation_fact", "bronze_observation"]),
                },
                SummaryCard {
                    label: "Privacy releases".to_string(),
                    value: pick_count(&counts, &["privacy_releases"]),
                },
                SummaryCard {
                    label: "Ingestion errors".to_string(),
                    value: pick_count(&counts, &["ingestion_errors"]),
                },
            ]);
        }
        DatabaseKind::OrchestratorLedger => {
            cards.extend([
                SummaryCard {
                    label: "Federated jobs".to_string(),
                    value: pick_count(&counts, &["federated_job_ledger"]),
                },
                SummaryCard {
                    label: "Release records".to_string(),
                    value: pick_count(&counts, &["federated_release_ledger"]),
                },
            ]);
        }
        DatabaseKind::Unknown => {}
    }

    cards
}

fn pick_count(counts: &HashMap<&str, u64>, candidates: &[&str]) -> String {
    candidates
        .iter()
        .find_map(|candidate| counts.get(candidate))
        .copied()
        .unwrap_or(0)
        .to_string()
}

fn detect_database_kind<'a>(relation_names: impl IntoIterator<Item = &'a str>) -> DatabaseKind {
    let names: Vec<&str> = relation_names.into_iter().collect();
    if names
        .iter()
        .any(|name| matches!(*name, "federated_job_ledger" | "federated_release_ledger"))
    {
        return DatabaseKind::OrchestratorLedger;
    }
    if names.iter().any(|name| {
        matches!(
            *name,
            "bronze_patient"
                | "patient_dim"
                | "condition_fact"
                | "medication_fact"
                | "observation_fact"
                | "feature_patient_summary"
                | "query_audit"
        )
    }) {
        return DatabaseKind::RefineryNode;
    }
    DatabaseKind::Unknown
}

fn categorize_relation(relation_name: &str) -> TableCategory {
    if relation_name.starts_with("bronze_") {
        TableCategory::Bronze
    } else if relation_name.starts_with("feature_") {
        TableCategory::Feature
    } else if relation_name.ends_with("_fact")
        || relation_name.ends_with("_dim")
        || relation_name == "quality_issues"
    {
        TableCategory::Core
    } else if relation_name.ends_with("_ledger") {
        TableCategory::Ledger
    } else if matches!(
        relation_name,
        "ingestion_errors" | "privacy_releases" | "query_audit" | "federated_job_audit"
    ) {
        TableCategory::Audit
    } else {
        TableCategory::Other
    }
}

fn canonical_data_dir(data_dir: &Path) -> ViewerResult<PathBuf> {
    data_dir
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize data directory {}",
                data_dir.display()
            )
        })
        .map_err(|error| ViewerError::internal(error.to_string()))
}

fn resolve_database_path(data_dir: &Path, requested_db: &str) -> ViewerResult<ResolvedDatabase> {
    if requested_db.trim().is_empty() {
        return Err(ViewerError::bad_request("database path must not be empty"));
    }

    let requested_path = Path::new(requested_db);
    if requested_path.is_absolute() {
        return Err(ViewerError::bad_request(
            "database path must be relative to the data directory",
        ));
    }

    let canonical_root = canonical_data_dir(data_dir)?;
    let candidate_path = canonical_root.join(requested_path);
    if !candidate_path.exists() {
        return Err(ViewerError::not_found(format!(
            "database '{requested_db}' does not exist"
        )));
    }

    let canonical_candidate = candidate_path
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize database path {}",
                candidate_path.display()
            )
        })
        .map_err(|error| ViewerError::internal(error.to_string()))?;

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(ViewerError::bad_request(
            "database path must stay inside the data directory",
        ));
    }
    if canonical_candidate
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("duckdb")
    {
        return Err(ViewerError::bad_request(
            "database path must point to a .duckdb file",
        ));
    }

    let metadata = read_file_metadata(&canonical_candidate)?;
    Ok(ResolvedDatabase {
        absolute_path: canonical_candidate.clone(),
        relative_path: strip_to_relative(&canonical_root, &canonical_candidate)?,
        file_name: canonical_candidate
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown.duckdb")
            .to_string(),
        size_bytes: metadata.size_bytes,
        modified_at: metadata.modified_at,
    })
}

fn strip_to_relative(root: &Path, absolute_path: &Path) -> ViewerResult<String> {
    let relative = absolute_path
        .strip_prefix(root)
        .with_context(|| {
            format!(
                "failed to strip {} from {}",
                root.display(),
                absolute_path.display()
            )
        })
        .map_err(|error| ViewerError::internal(error.to_string()))?;
    Ok(display_path(relative))
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn render_value(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Boolean(value) => value.to_string(),
        ValueRef::TinyInt(value) => value.to_string(),
        ValueRef::SmallInt(value) => value.to_string(),
        ValueRef::Int(value) => value.to_string(),
        ValueRef::BigInt(value) => value.to_string(),
        ValueRef::HugeInt(value) => value.to_string(),
        ValueRef::UTinyInt(value) => value.to_string(),
        ValueRef::USmallInt(value) => value.to_string(),
        ValueRef::UInt(value) => value.to_string(),
        ValueRef::UBigInt(value) => value.to_string(),
        ValueRef::Float(value) => value.to_string(),
        ValueRef::Double(value) => value.to_string(),
        ValueRef::Decimal(value) => value.to_string(),
        ValueRef::Timestamp(unit, raw) => format_timestamp(unit, raw),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Blob(bytes) => format!("<{} bytes>", bytes.len()),
        ValueRef::Date32(days_since_epoch) => format_date(days_since_epoch),
        ValueRef::Time64(_, micros) => format_time(micros),
        ValueRef::Interval {
            months,
            days,
            nanos,
        } => format!("{months} months, {days} days, {nanos} ns"),
        other => format!("{:?}", other.to_owned()),
    }
}

fn format_timestamp(unit: duckdb::types::TimeUnit, raw: i64) -> String {
    let (seconds, nanos) = match unit {
        duckdb::types::TimeUnit::Second => (raw, 0),
        duckdb::types::TimeUnit::Millisecond => (
            raw.div_euclid(1_000),
            (raw.rem_euclid(1_000) * 1_000_000) as u32,
        ),
        duckdb::types::TimeUnit::Microsecond => (
            raw.div_euclid(1_000_000),
            (raw.rem_euclid(1_000_000) * 1_000) as u32,
        ),
        duckdb::types::TimeUnit::Nanosecond => (
            raw.div_euclid(1_000_000_000),
            raw.rem_euclid(1_000_000_000) as u32,
        ),
    };

    DateTime::<Utc>::from_timestamp(seconds, nanos)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S%.6f")
                .to_string()
        })
        .unwrap_or_else(|| raw.to_string())
}

fn format_date(days_since_epoch: i32) -> String {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .map(|epoch| epoch + Duration::days(days_since_epoch as i64))
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| days_since_epoch.to_string())
}

fn format_time(micros_since_midnight: i64) -> String {
    let seconds = micros_since_midnight.div_euclid(1_000_000) as u32;
    let micros = micros_since_midnight.rem_euclid(1_000_000) as u32;
    NaiveTime::from_num_seconds_from_midnight_opt(seconds, micros * 1_000)
        .map(|time| time.format("%H:%M:%S%.6f").to_string())
        .unwrap_or_else(|| micros_since_midnight.to_string())
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn read_file_metadata(path: &Path) -> ViewerResult<FileMetadata> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))
        .map_err(|error| ViewerError::internal(error.to_string()))?;
    let modified_at = metadata.modified().ok().map(DateTime::<Local>::from);

    Ok(FileMetadata {
        size_bytes: metadata.len(),
        modified_at,
    })
}

#[derive(Debug, Clone)]
struct FileMetadata {
    size_bytes: u64,
    modified_at: Option<DateTime<Local>>,
}

#[derive(Debug, Clone)]
struct ResolvedDatabase {
    absolute_path: PathBuf,
    relative_path: String,
    file_name: String,
    size_bytes: u64,
    modified_at: Option<DateTime<Local>>,
}

#[cfg(test)]
mod tests {
    use super::{categorize_relation, detect_database_kind, quote_identifier};
    use crate::models::{DatabaseKind, TableCategory};

    #[test]
    fn quote_identifier_escapes_embedded_quotes() {
        assert_eq!(quote_identifier("bad\"name"), "\"bad\"\"name\"");
    }

    #[test]
    fn detect_node_database_from_core_tables() {
        assert_eq!(
            detect_database_kind(["patient_dim", "condition_fact"]),
            DatabaseKind::RefineryNode
        );
    }

    #[test]
    fn detect_orchestrator_database_from_ledger_tables() {
        assert_eq!(
            detect_database_kind(["federated_job_ledger"]),
            DatabaseKind::OrchestratorLedger
        );
    }

    #[test]
    fn categorize_known_relation_groups() {
        assert_eq!(categorize_relation("bronze_patient"), TableCategory::Bronze);
        assert_eq!(categorize_relation("condition_fact"), TableCategory::Core);
        assert_eq!(
            categorize_relation("feature_patient_summary"),
            TableCategory::Feature
        );
        assert_eq!(categorize_relation("query_audit"), TableCategory::Audit);
        assert_eq!(
            categorize_relation("federated_release_ledger"),
            TableCategory::Ledger
        );
    }
}
