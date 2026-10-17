use serde::{Deserialize, Serialize};

// ============ Scan ============

#[derive(Serialize, Clone)]
pub struct ScanReport {
    pub data_root: String,
    pub cache_root: String,
    pub total_bytes: u64,
    pub total_mb: f64,
    pub directories: Vec<DirUsage>,
    pub sessions: Vec<SessionUsage>,
    pub session_total_bytes: u64,
    pub scanned_at: i64,
    pub opencode_version: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct DirUsage {
    pub key: DirKey,
    pub label: String,
    pub path: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub safe_while_running: bool,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "PascalCase")]
pub enum DirKey {
    Log,
    SessionDiff,
    Snapshot,
    ToolOutput,
    Repos,
    CachePackages,
    CacheBin,
    CacheModels,
}

#[derive(Serialize, Clone)]
pub struct SessionUsage {
    pub id: String,
    pub title: String,
    pub project_id: String,
    pub project_name: Option<String>,
    pub directory: Option<String>,
    pub agent: Option<String>,
    pub model_id: Option<String>,
    pub created_at: i64,
    pub last_active: i64,
    pub size_bytes: u64,
    pub part_count: u64,
    pub message_count: u64,
    pub has_diff: bool,
    pub cost: f64,
    pub archived: bool,
}

// ============ Clean ============

#[derive(Serialize, Deserialize, Clone)]
pub struct CleanSelection {
    #[serde(default)]
    pub session_ids: Vec<String>,
    pub keep_recent: Option<u32>,
    pub older_than_days: Option<u32>,
    pub min_size_mb: Option<f64>,
    #[serde(default)]
    pub project_ids: Vec<String>,
    #[serde(default)]
    pub directories: Vec<DirKeySel>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "PascalCase")]
pub enum DirKeySel {
    Log,
    SessionDiff,
    Snapshot,
    ToolOutput,
    Repos,
    CachePackages,
    CacheBin,
    CacheModels,
}

impl DirKeySel {
    pub fn label(&self) -> &'static str {
        match self {
            DirKeySel::Log => "Logs",
            DirKeySel::SessionDiff => "Session Diffs",
            DirKeySel::Snapshot => "Snapshots",
            DirKeySel::ToolOutput => "Tool Outputs",
            DirKeySel::Repos => "Repos",
            DirKeySel::CachePackages => "Model Packages Cache",
            DirKeySel::CacheBin => "Binaries Cache",
            DirKeySel::CacheModels => "Models Cache",
        }
    }
}

#[derive(Serialize)]
pub struct CleanPreview {
    pub items: Vec<CleanPreviewItem>,
    pub total_freed_bytes: u64,
    pub total_freed_mb: f64,
    pub affected_session_count: u32,
    pub backup_estimated_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct CleanPreviewItem {
    pub category: CleanCategory,
    pub label: String,
    pub size_bytes: u64,
    pub count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum CleanCategory {
    SessionData,
    SessionDiff,
    Snapshot,
    Log,
    Cache,
    ToolOutput,
    Repos,
}

#[derive(Serialize)]
pub struct CleanResult {
    pub freed_bytes: u64,
    pub freed_mb: f64,
    pub backup_path: Option<String>,
    pub errors: Vec<CleanError>,
    pub vacuum_reclaimed_bytes: u64,
    pub duration_ms: u64,
}

#[derive(Serialize)]
pub struct CleanError {
    pub item: String,
    pub message: String,
}

// ============ Process ============

#[derive(Serialize, Clone)]
pub struct ProcessStatus {
    pub running: bool,
    pub processes: Vec<String>,
}

// ============ Backup ============

#[derive(Serialize, Clone)]
pub struct BackupInfo {
    pub id: String,
    pub created_at: i64,
    pub size_bytes: u64,
    pub session_count: u32,
    pub label: String,
}

// ============ Settings ============

#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub language: String,
    pub default_keep_recent: u32,
    pub backup_retention: u32,
    pub auto_update_check: bool,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "zh".to_string(),
            default_keep_recent: 50,
            backup_retention: 5,
            auto_update_check: true,
            theme: "dark".to_string(),
        }
    }
}
