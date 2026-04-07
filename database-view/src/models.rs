use chrono::{DateTime, Local};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseKind {
    RefineryNode,
    OrchestratorLedger,
    Unknown,
}

impl DatabaseKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::RefineryNode => "Refinery node",
            Self::OrchestratorLedger => "Orchestrator ledger",
            Self::Unknown => "DuckDB file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TableCategory {
    Bronze,
    Core,
    Feature,
    Audit,
    Ledger,
    Other,
}

impl TableCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bronze => "Bronze",
            Self::Core => "Core",
            Self::Feature => "Feature",
            Self::Audit => "Audit",
            Self::Ledger => "Ledger",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Table,
    View,
}

impl RelationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Table => "Table",
            Self::View => "View",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseListPage {
    pub data_dir_display: String,
    pub databases: Vec<DatabaseListEntry>,
}

#[derive(Debug, Clone)]
pub struct DatabaseListEntry {
    pub relative_path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Local>>,
    pub kind: DatabaseKind,
    pub relation_count: Option<usize>,
    pub inspection_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SummaryCard {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct RelationSummary {
    pub name: String,
    pub relation_kind: RelationKind,
    pub category: TableCategory,
    pub row_count: u64,
    pub column_count: usize,
}

#[derive(Debug, Clone)]
pub struct DatabaseOverview {
    pub relative_path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Local>>,
    pub kind: DatabaseKind,
    pub cards: Vec<SummaryCard>,
    pub relations: Vec<RelationSummary>,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone)]
pub struct TablePage {
    pub database_relative_path: String,
    pub database_file_name: String,
    pub database_kind: DatabaseKind,
    pub relation_name: String,
    pub relation_kind: RelationKind,
    pub category: TableCategory,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<String>>,
    pub page: usize,
    pub page_size: usize,
    pub total_rows: u64,
    pub has_previous_page: bool,
    pub has_next_page: bool,
}
