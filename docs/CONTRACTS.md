# CONTRACTS: opencode-cleaner Core Engine

本文档定义 Rust 核心引擎与前端之间的接口契约。实现以此为准，测试覆盖每条命令。所有 JSON 字段为 snake_case。

---

## 1. Data Types

```rust
use serde::{Serialize, Deserialize};

// ============ Scan ============

#[derive(Serialize)]
pub struct ScanReport {
    pub data_root: String,
    pub cache_root: String,
    pub total_bytes: u64,
    pub total_mb: f64,
    pub directories: Vec<DirUsage>,
    pub sessions: Vec<SessionUsage>,
    pub session_total_bytes: u64,
    pub scanned_at: i64,                 // Unix ms
    pub opencode_version: Option<String>,// 探测安装版本
}

#[derive(Serialize)]
pub struct DirUsage {
    pub key: DirKey,                     // 枚举，见下
    pub label: String,                   // i18n key 由前端解析
    pub path: String,                    // 绝对路径
    pub size_bytes: u64,
    pub file_count: u64,
    pub safe_while_running: bool,
}

#[derive(Serialize)]
pub enum DirKey {
    Log,             // data/log
    SessionDiff,     // data/storage/session_diff
    Snapshot,        // data/snapshot (项目级聚合)
    ToolOutput,      // data/tool-output
    Repos,           // data/repos
    CachePackages,   // cache/packages
    CacheBin,        // cache/bin
    CacheModels,     // cache/models.json
}

#[derive(Serialize)]
pub struct SessionUsage {
    pub id: String,                      // ses_xxx
    pub title: String,
    pub project_id: String,
    pub project_name: Option<String>,
    pub directory: Option<String>,
    pub agent: Option<String>,
    pub model_id: Option<String>,        // "provider/model"
    pub created_at: i64,                 // time_created (Unix ms)
    pub last_active: i64,                // time_updated (Unix ms)
    pub size_bytes: u64,                 // SUM(LENGTH(part.data)) + message + todo 估算
    pub part_count: u64,
    pub message_count: u64,
    pub has_diff: bool,                  // storage/session_diff/ses_xxx.json 存在
    pub cost: f64,
    pub archived: bool,                  // time_archived IS NOT NULL
}

// ============ Clean ============

#[derive(Serialize, Deserialize)]
pub struct CleanSelection {
    pub session_ids: Vec<String>,            // 显式指定
    pub keep_recent: Option<u32>,            // 替代：保留最近 N 个，其余全选
    pub older_than_days: Option<u32>,        // 过滤：仅 N 天前
    pub min_size_mb: Option<f64>,            // 过滤：仅 ≥ X MB
    pub project_ids: Vec<String>,            // 过滤：限定项目
    pub directories: Vec<DirKey>,            // 目录类清理
    // 互斥规则：keep_recent 与 session_ids 二选一；older_than/min_size/project 作为筛选叠加
}

#[derive(Serialize)]
pub struct CleanPreview {
    pub items: Vec<CleanPreviewItem>,
    pub total_freed_bytes: u64,
    pub total_freed_mb: f64,
    pub affected_session_count: u32,
    pub backup_estimated_bytes: u64,
    pub warnings: Vec<String>,               // 如「将丢失 N 个项目的 undo」
}

#[derive(Serialize)]
pub struct CleanPreviewItem {
    pub category: CleanCategory,
    pub label: String,
    pub size_bytes: u64,
    pub count: u32,                          // 会话数/文件数
}

#[derive(Serialize)]
pub enum CleanCategory {
    SessionData,    // DB 行（session 级联 + event_sequence）
    SessionDiff,    // storage/session_diff/ses_xxx.json
    Snapshot,       // snapshot/<project>/（项目级）
    Log,
    Cache,
    ToolOutput,
    Repos,
}

#[derive(Serialize)]
pub struct CleanResult {
    pub freed_bytes: u64,        // 实际回收字节：目录按实际删除的文件大小累计，
                                 // 会话按 DB 中 part 文本长度计（内容口径）
    pub freed_mb: f64,
    pub backup_path: Option<String>,
    pub errors: Vec<CleanError>, // 部分失败条目（item = 目录 key 或 "database"）
    pub vacuum_reclaimed_bytes: u64, // VACUUM 前后 DB 文件字节差
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
    pub processes: Vec<String>,             // ["opencode", "opencode-desktop"]
}

// ============ Backup ============

#[derive(Serialize)]
pub struct BackupInfo {
    pub id: String,                         // timestamp
    pub created_at: i64,
    pub size_bytes: u64,
    pub session_count: u32,
    pub label: String,                      // 用户可读摘要
}

// ============ Settings ============

#[derive(Serialize, Deserialize)]
pub struct AppSettings {
    pub language: String,                   // "zh" | "en"
    pub default_keep_recent: u32,           // 默认保留最近 N 会话
    pub backup_retention: u32,              // 默认 5
    pub auto_update_check: bool,
    pub theme: String,                      // "dark" | "light" | "system"
}
```

---

## 2. Tauri Commands (Rust → Frontend)

| Command          | Args                  | Returns                  | Side Effect | 进度事件 |
| ---------------- | --------------------- | ------------------------ | ----------- | -------- |
| `scan`           | —                     | `ScanReport`             | 无          | `scan-progress` |
| `dry_run`        | `CleanSelection`      | `CleanPreview`           | 无          | — |
| `execute`        | `CleanSelection`      | `CleanResult`            | 备份+删除+VACUUM | `clean-progress` |
| `check_process`  | —                     | `ProcessStatus`          | 无          | 周期 `process-status` |
| `get_backups`    | —                     | `Vec<BackupInfo>`        | 无          | — |
| `restore`        | `backup_id: String`   | `()`                     | 覆盖 DB+目录 | `restore-progress` |
| `delete_backup`  | `backup_id: String`   | `()`                     | 删备份      | — |
| `get_settings`   | —                     | `AppSettings`            | 无          | — |
| `set_settings`   | `AppSettings`         | `AppSettings`            | 写 JSON     | — |
| `get_data_root`  | —                     | `String`                 | 无          | — |
| `export_diagnostics` | —                | `String` (文件路径)      | 写 zip      | — |

### 命令签名（Rust）

```rust
#[tauri::command]
async fn scan(app: AppHandle) -> Result<ScanReport, String>;

#[tauri::command]
async fn dry_run(selection: CleanSelection) -> Result<CleanPreview, String>;

#[tauri::command]
async fn execute(app: AppHandle, selection: CleanSelection)
    -> Result<CleanResult, String>;

#[tauri::command]
async fn check_process() -> Result<ProcessStatus, String>;

#[tauri::command]
async fn get_backups() -> Result<Vec<BackupInfo>, String>;

#[tauri::command]
async fn restore(app: AppHandle, backup_id: String) -> Result<(), String>;

#[tauri::command]
async fn get_settings() -> Result<AppSettings, String>;

#[tauri::command]
async fn set_settings(settings: AppSettings) -> Result<AppSettings, String>;

#[tauri::command]
async fn get_data_root() -> Result<String, String>;

#[tauri::command]
async fn export_diagnostics(app: AppHandle) -> Result<String, String>;
```

---

## 3. Event Payloads

```ts
// scan-progress
{ "dir": "log", "current": 3, "total": 8, "label_key": "dir.log" }

// clean-progress
{ "phase": "backup" | "deleting" | "vacuum" | "files",
  "current": number, "total": number, "label": string }

// process-status
{ "running": boolean, "processes": string[] }

// restore-progress
{ "current": number, "total": number }
```

---

## 4. Request / Response Examples

### scan
```jsonc
// → invoke("scan")
// ←
{
  "data_root": "/Users/x/.local/share/opencode",
  "cache_root": "/Users/x/.cache/opencode",
  "total_bytes": 20971520,
  "total_mb": 20.0,
  "directories": [
    { "key": "Log", "label": "Log", "path": ".../log", "size_bytes": 3145728, "file_count": 12, "safe_while_running": true },
    { "key": "Snapshot", "label": "Snapshot", "path": ".../snapshot", "size_bytes": 630784, "file_count": 46, "safe_while_running": true },
    { "key": "CachePackages", "label": "CachePackages", "path": ".../packages", "size_bytes": 75497472, "file_count": 3, "safe_while_running": true }
  ],
  "sessions": [
    {
      "id": "ses_1c99e14f3ffez6I7iX8FtHL7mL",
      "title": "不使用 Docker 搭建项目环境",
      "project_id": "prj_xxx",
      "project_name": "cross-repo-graph",
      "directory": "/Users/x/data/work/cross-repo-graph",
      "agent": "build",
      "model_id": "anthropic/claude-sonnet-4",
      "created_at": 1747635600000,
      "last_active": 1747722000000,
      "size_bytes": 953548,
      "part_count": 312,
      "message_count": 48,
      "has_diff": true,
      "cost": 0.42,
      "archived": false
    }
  ],
  "session_total_bytes": 11534336,
  "scanned_at": 1722528000000,
  "opencode_version": "1.17.7"
}
```

### dry_run → execute
```jsonc
// → invoke("dry_run", { selection: {
//     "session_ids": ["ses_1c99e14f3ffez6I7iX8FtHL7mL"],
//     "keep_recent": null, "older_than_days": null,
//     "min_size_mb": null, "project_ids": [],
//     "directories": ["Log","CachePackages"] }})
// ←
{
  "items": [
    { "category": "SessionData",  "label": "ses_1c99e14f...", "size_bytes": 953548, "count": 1 },
    { "category": "Log",          "label": "log/*.log",       "size_bytes": 3145728, "count": 12 },
    { "category": "Cache",        "label": "cache/packages",  "size_bytes": 75497472, "count": 3 }
  ],
  "total_freed_bytes": 79564748,
  "total_freed_mb": 75.88,
  "affected_session_count": 1,
  "backup_estimated_bytes": 12582912,
  "warnings": ["将丢失 1 个会话的 undo（session_diff）"]
}

// → invoke("execute", { selection })   // 同上
// ←
{
  "freed_bytes": 79564748,
  "freed_mb": 75.88,
  "backup_path": "/Users/x/.local/share/opencode-cleaner/backups/20260801-230000",
  "errors": [],
  "vacuum_reclaimed_bytes": 1048576,
  "duration_ms": 3420
}
```

---

## 5. Internal DB Contracts (Rust-only, 不暴露)

```rust
// db.rs

/// 只读连接：扫描/聚合用
fn open_readonly(data_root: &str) -> Result<Connection>;
// PRAGMA query_only=1; PRAGMA journal_mode=WAL 不改

/// 读写连接：删除/VACUUM 用（调用方保证 opencode 未运行）
fn open_readwrite(data_root: &str) -> Result<Connection>;
// PRAGMA foreign_keys=ON   ← 硬性：级联依赖此 pragma

/// 会话占用聚合
fn scan_sessions(conn: &Connection) -> Result<Vec<SessionUsage>>;
// SELECT s.id, s.title, s.project_id, p.name AS project_name,
//        s.agent, s.model, s.time_created, s.time_updated, s.cost,
//        s.time_archived,
//        SUM(LENGTH(part.data)) AS size,
//        COUNT(DISTINCT part.id) AS part_count,
//        COUNT(DISTINCT m.id) AS message_count
// FROM session s
// LEFT JOIN message m ON m.session_id = s.id
// LEFT JOIN part ON part.session_id = s.id
// LEFT JOIN project p ON p.id = s.project_id
// GROUP BY s.id
// ORDER BY size DESC
// 注：part.size 含 tool 全文（webfetch/bash/read/write）+ reasoning + text

/// 删除会话（事务内）
fn delete_sessions(tx: &Transaction, ids: &[String]) -> Result<u64>;
// 1. DELETE FROM session WHERE id IN (?)        -- 级联 message→part, todo,
//                                                 --    session_share, session_message,
//                                                 --    session_input, session_context_epoch
// 2. DELETE FROM event_sequence WHERE aggregate_id IN (?)  -- 级联 event（无 session FK）
// 返回受影响行数

/// VACUUM（事务外，独占）
fn vacuum(conn: &mut Connection) -> Result<u64>;
// VACUUM; 返回前 PRAGMA freelist_count 差值 × page_size

/// WAL 落盘（备份前）
fn checkpoint(conn: &Connection) -> Result<()>;
// PRAGMA wal_checkpoint(TRUNCATE)
```

### Schema 依赖声明
- **稳定读取列**：`session(id,title,project_id,agent,model,time_created,time_updated,cost,time_archived,directory)`、`message(id,session_id)`、`part(id,session_id,data)`、`project(id,name)`
- **探测降级**：`agent`/`model`/`cost`/`directory`/`time_archived` 为较新列，读取前 `PRAGMA table_info` 探测，缺失则字段返回 None
- **凭证禁区（不读不写）**：`account`、`account_state`、`control_account`、`credential`、`auth.json`

---

## 6. Internal Filesystem Contracts (Rust-only)

```rust
// fs.rs

struct DataPaths { data_root: PathBuf, cache_root: PathBuf }

fn resolve_paths() -> Result<DataPaths>;
// data  = env::XDG_DATA_HOME || ~/.local/share  → join "opencode"
// cache = env::XDG_CACHE_HOME || ~/.cache       → join "opencode"
// 探测 data_root/opencode.db 存在；缺失 → Err(DataDirNotFound)
//
// !!! 平台一致性：以上规则在 macOS / Windows / Linux 上完全相同。
// opencode 自身并未对 Windows 做 AppData 特判——它在 Windows 上同样读取
// XDG_DATA_HOME，未设置时回落到 %USERPROFILE%\.local\share\opencode。
// 因此 Windows 下 data_root = C:\Users\<user>\.local\share\opencode，
// **不是** %APPDATA% 或 %LOCALAPPDATA%。切勿按「Windows 惯例」改写此处。

fn dir_usage(path: &Path) -> Result<DirUsage>;             // 递归 du + count
fn remove_dir_all_counted(path: &Path) -> (u64, Vec<String>); // 快照/缓存整目录
// 返回 (实际删除字节, 失败条目)。逐文件删除、失败不中断、如实记账：
// Windows 强制文件锁下单个被占用文件会让 fs::remove_dir_all 整棵失败，
// 此处则保留其余可删文件并把失败项上报（见 CleanResult.errors）。
fn remove_glob(path: &Path, pattern: &str) -> Result<u64>; // session_diff/ses_*.json，返回删除数
fn archive_dir(src: &Path, dst: &Path) -> Result<u64>;     // tar.gz，返回大小

fn free_space(path: &Path) -> u64;                          // 按路径所在卷，失败返回 0
// Windows: GetDiskFreeSpaceExW；Unix: df -k <path>
// 调用方在返回 0 时跳过空间预检（该预检非关键路径）。

// 会话 diff 路径
fn session_diff_path(root: &Path, sid: &str) -> PathBuf;
// root/storage/session_diff/{sid}.json

// 快照：项目级
fn snapshot_dir(root: &Path, project_id: &str) -> PathBuf;
// root/snapshot/{project_id}/   ← 含多个 worktree hash 子目录
// 清理粒度：整个 project_id 目录（跨会话共享，不支持按会话删）
```

---

## 6.1 Relocation Contract (Windows only, ADR-010)

```rust
// migrate.rs

struct StorageStatus {
    platform, supported, data_root, cache_root,
    data_real_path, cache_real_path,      // 解析联接后的真实位置（剥掉 \\?\ 前缀）
    data_relocated, cache_relocated,      // 该路径自身带 FILE_ATTRIBUTE_REPARSE_POINT
    data_bytes, cache_bytes,
    pending_backup_bytes, pending_backup_paths,  // 旧卷上待手动删除的副本
    state_dir, opencode_running,
}

fn status() -> StorageStatus;                    // 三平台通用；非 Windows 时 supported=false
fn preflight(target_base: &Path) -> Preflight;  // ok / blockers / warnings / free_bytes / junction_supported
fn execute(target_base: &Path, on_progress) -> MigrationReport;  // windows
fn undo() -> Vec<String>;                       // 撤销：摘联接 + 旧副本换回
fn discard_backup() -> Vec<String>;             // 删除旧副本（不可撤销，独立命令）
```

契约要点：

- **不改环境变量、不改 opencode 路径**。原路径变为 NTFS 目录联接（`mklink /J`），
  所有解析旧路径的消费者自动落到新卷。
- **迁移范围**：`data`（会话/日志/快照/tool-output）+ `cache`（模型包/二进制）。
  `state` 不迁移。
- **布局**：`<target_base>/data/opencode` 与 `<target_base>/cache/opencode`。
- **切换前硬门禁**：源/目标文件数一致、字节数一致、目标 DB 可打开且
  `session_count` 与源相同；任一不符 → 中止，不切换。
- **失败语义**：建联接失败 → 原目录自动 rename 回原位并返回错误；
  绝不出现「原目录已移走且联接没建成」的状态。
- **旧副本**：`<root>.migrated-backup-<ts>`，迁移流程永不删除；
  系统盘空间只由 `discard_backup()` 释放。
- **重复迁移**：preflight 检测到已有联接或遗留旧副本时拒绝，并提示先 `undo()`。
- **联接探测**：预检阶段在「源卷父目录」与「目标目录」各建一个探针联接再删除，
  实测两个卷是否都支持；exFAT/FAT32 会在预检就被拒绝。
- **缺失的根跳过**：某个根不存在（全新安装、从未用过缓存）时跳过并在
  `MigrationReport.warnings` 中说明，不影响另一个根的迁移。
- **链接/特殊文件即中止**：数据目录内若存在 symlink/junction/fifo，复制阶段直接报错
  （而非静默跳过——跳过会让后续校验以「文件数不一致」这种难以理解的形式失败）。
  校验失败发生在切换之前，系统此时未被改动。

---

## 7. Internal Process Contract

```rust
// process.rs
fn check() -> ProcessStatus;
// sysinfo::System::processes_by_name 遍历，匹配（大小写不敏感）：
//   "opencode" | "opencode-desktop"
// Windows 进程名带 .exe，统一 strip
```

---

## 8. Error Enum

```rust
#[derive(Debug, thiserror::Error, Serialize)]
pub enum CleanerError {
    #[error("opencode is running: {0:?}")]
    ProcessRunning(Vec<String>),

    #[error("data directory not found: {0}")]
    DataDirNotFound(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(String),                          // 不直接 from io::Error，序列化友好

    #[error("backup failed: {0}")]
    BackupFailed(String),

    #[error("restore failed: {0}")]
    RestoreFailed(String),

    #[error("insufficient disk space: need {needed}, have {available}")]
    InsufficientSpace { needed: u64, available: u64 },

    #[error("incompatible opencode schema: {0}")]
    IncompatibleSchema(String),

    #[error("invalid selection: {0}")]
    InvalidSelection(String),
}
```

---

## 9. Test Contracts

| ID     | 测试                                                       | 覆盖契约 |
| ------ | --------------------------------------------------------- | -------- |
| T-1    | mock 数据全量扫描，总量 = 各目录 + 会话总和                | scan     |
| T-2    | dry_run 空选返回 0 字节                                    | dry_run  |
| T-3    | dry_run 指定会话返回精确 part.data 字节和                  | dry_run  |
| T-4    | execute 在 opencode 运行时返回 ProcessRunning              | execute  |
| T-5    | execute 删除 N 会话后 session/message/part/event 全无残留  | execute  |
| T-6    | execute 后 event_sequence 无 aggregate_id 残留（孤儿清理） | execute  |
| T-7    | execute 后 session_diff/ses_xxx.json 已删                  | execute  |
| T-8    | execute 后 freelist 下降（VACUUM 生效）                    | execute  |
| T-9    | backup → restore 往返：DB 行数与目录一致                   | restore  |
| T-10   | 备份超 5 份自动删最旧                                      | backup   |
| T-11   | 磁盘不足时 execute 返回 InsufficientSpace（fail-closed）  | execute  |
| T-12   | 缺失 model/agent 列时 scan 不崩，字段为 None               | scan     |
| T-13   | keep_recent=N 时选中除最近 N 外全部                        | dry_run  |
| T-14   | 进程名大小写/带 .exe 均能识别                              | process  |

夹具：`test/fixtures/seed.sql` 构造 5 会话 / 100 part 固定数据；`test/fixtures/fs/` 模拟目录树。
