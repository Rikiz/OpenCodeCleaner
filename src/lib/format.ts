import type { DirKey } from "./types";

export function formatBytes(bytes: number, digits = 1): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const val = bytes / Math.pow(1024, i);
  const d = i === 0 ? 0 : digits;
  return `${val.toFixed(d)} ${units[i]}`;
}

export function formatMB(mb: number): string {
  return formatBytes(mb * 1024 * 1024);
}

export function formatTime(ms: number): string {
  if (!ms) return "—";
  const d = new Date(ms);
  const now = new Date();
  const diff = now.getTime() - ms;
  const day = 86_400_000;
  if (diff < day && d.getDate() === now.getDate()) {
    return d.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  }
  if (diff < 7 * day) {
    return d.toLocaleDateString("zh-CN", { month: "short", day: "numeric" });
  }
  return d.toLocaleDateString("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit" });
}

export function relativeTime(ms: number): string {
  if (!ms) return "—";
  const diff = Date.now() - ms;
  const min = 60_000, hr = 3_600_000, day = 86_400_000;
  if (diff < min) return "刚刚";
  if (diff < hr) return `${Math.floor(diff / min)} 分钟前`;
  if (diff < day) return `${Math.floor(diff / hr)} 小时前`;
  if (diff < 30 * day) return `${Math.floor(diff / day)} 天前`;
  return formatTime(ms);
}

const DIR_PALETTE: Record<DirKey, string> = {
  Log: "#7c5cff",
  SessionDiff: "#4cc2ff",
  Snapshot: "#34d399",
  ToolOutput: "#fbbf24",
  Repos: "#fb7185",
  CachePackages: "#a78bfa",
  CacheBin: "#22d3ee",
  CacheModels: "#f472b6",
};

export function dirColor(key: DirKey): string {
  return DIR_PALETTE[key] ?? "#888";
}

const DIR_LABELS: Record<DirKey, string> = {
  Log: "日志",
  SessionDiff: "会话 diff",
  Snapshot: "快照",
  ToolOutput: "工具输出",
  Repos: "仓库缓存",
  CachePackages: "模型包缓存",
  CacheBin: "二进制缓存",
  CacheModels: "模型列表缓存",
};

export function dirLabel(key: DirKey): string {
  return DIR_LABELS[key] ?? key;
}

export const DIR_ORDER: DirKey[] = [
  "Log", "SessionDiff", "Snapshot", "ToolOutput", "Repos",
  "CachePackages", "CacheBin", "CacheModels",
];
