import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  BackupInfo,
  CleanPreview,
  CleanResult,
  CleanSelection,
  MigrationReport,
  Preflight,
  ProcessStatus,
  ScanReport,
  StorageStatus,
} from "./types";

export const api = {
  scan: () => invoke<ScanReport>("scan"),
  dryRun: (selection: CleanSelection) => invoke<CleanPreview>("dry_run", { selection }),
  execute: (selection: CleanSelection) => invoke<CleanResult>("execute", { selection }),
  checkProcess: () => invoke<ProcessStatus>("check_process"),
  getBackups: () => invoke<BackupInfo[]>("get_backups"),
  restore: (backupId: string) => invoke<void>("restore", { backupId }),
  deleteBackup: (backupId: string) => invoke<void>("delete_backup", { backupId }),
  getSettings: () => invoke<AppSettings>("get_settings"),
  setSettings: (settings: AppSettings) => invoke<AppSettings>("set_settings", { settings }),
  getDataRoot: () => invoke<string>("get_data_root"),
  exportDiagnostics: () => invoke<string>("export_diagnostics"),
  storageInfo: () => invoke<StorageStatus>("storage_info"),
  migratePreflight: (targetBase: string) =>
    invoke<Preflight>("migrate_preflight", { targetBase }),
  migrateExecute: (targetBase: string) =>
    invoke<MigrationReport>("migrate_execute", { targetBase }),
  migrateUndo: () => invoke<string[]>("migrate_undo"),
  migrateDiscardBackup: () => invoke<string[]>("migrate_discard_backup"),
};

export interface ScanProgress {
  dir: string;
  current: number;
  total: number;
  label: string;
}
export interface CleanProgress {
  phase: string;
  current: number;
  total: number;
}

export async function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan-progress", (e) => cb(e.payload));
}

export async function onCleanProgress(cb: (p: CleanProgress) => void): Promise<UnlistenFn> {
  return listen<CleanProgress>("clean-progress", (e) => cb(e.payload));
}

export async function onRestoreDone(cb: () => void): Promise<UnlistenFn> {
  return listen("restore-done", () => cb());
}

export interface MigrateProgress {
  phase: string;
  item: string;
  current: number;
  total: number;
  bytes: number;
}

export async function onMigrateProgress(
  cb: (p: MigrateProgress) => void,
): Promise<UnlistenFn> {
  return listen<MigrateProgress>("migrate-progress", (e) => cb(e.payload));
}
