// Types mirrored from Rust src/types.rs (CONTRACTS §1).

export type DirKey =
  | "Log" | "SessionDiff" | "Snapshot" | "ToolOutput" | "Repos"
  | "CachePackages" | "CacheBin" | "CacheModels";

export type DirKeySel = DirKey;

export interface ScanReport {
  data_root: string;
  cache_root: string;
  total_bytes: number;
  total_mb: number;
  directories: DirUsage[];
  sessions: SessionUsage[];
  session_total_bytes: number;
  scanned_at: number;
  opencode_version: string | null;
}

export interface DirUsage {
  key: DirKey;
  label: string;
  path: string;
  size_bytes: number;
  file_count: number;
  safe_while_running: boolean;
}

export interface SessionUsage {
  id: string;
  title: string;
  project_id: string;
  project_name: string | null;
  directory: string | null;
  agent: string | null;
  model_id: string | null;
  created_at: number;
  last_active: number;
  size_bytes: number;
  part_count: number;
  message_count: number;
  has_diff: boolean;
  cost: number;
  archived: boolean;
}

export interface CleanSelection {
  session_ids: string[];
  keep_recent: number | null;
  older_than_days: number | null;
  min_size_mb: number | null;
  project_ids: string[];
  directories: DirKeySel[];
}

export type CleanCategory =
  | "SessionData" | "SessionDiff" | "Snapshot" | "Log" | "Cache"
  | "ToolOutput" | "Repos";

export interface CleanPreviewItem {
  category: CleanCategory;
  label: string;
  size_bytes: number;
  count: number;
}

export interface CleanPreview {
  items: CleanPreviewItem[];
  total_freed_bytes: number;
  total_freed_mb: number;
  affected_session_count: number;
  backup_estimated_bytes: number;
  warnings: string[];
}

export interface CleanResult {
  freed_bytes: number;
  freed_mb: number;
  backup_path: string | null;
  errors: { item: string; message: string }[];
  vacuum_reclaimed_bytes: number;
  duration_ms: number;
}

export interface ProcessStatus {
  running: boolean;
  processes: string[];
}

export interface BackupInfo {
  id: string;
  created_at: number;
  size_bytes: number;
  session_count: number;
  label: string;
}

export interface AppSettings {
  language: string;
  default_keep_recent: number;
  backup_retention: number;
  auto_update_check: boolean;
  theme: string;
}

export type View = "dashboard" | "sessions" | "directories" | "backups" | "settings";
