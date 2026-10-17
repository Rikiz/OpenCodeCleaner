import { create } from "zustand";
import { api, onScanProgress, onCleanProgress, type ScanProgress, type CleanProgress } from "./lib/invoke";
import type {
  AppSettings,
  CleanPreview,
  ProcessStatus,
  ScanReport,
  View,
} from "./lib/types";

interface State {
  view: View;
  report: ScanReport | null;
  loading: boolean;
  scanLabel: string;
  process: ProcessStatus | null;
  settings: AppSettings | null;
  cleanPreview: CleanPreview | null;
  cleanBusy: boolean;
  cleanPhase: CleanProgress | null;
  cleanError: string | null;

  setView: (v: View) => void;
  refresh: () => Promise<void>;
  refreshProcess: () => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSettings: (s: AppSettings) => Promise<void>;
  setCleanPreview: (p: CleanPreview | null) => void;
  runClean: (selection: import("./lib/types").CleanSelection) => Promise<void>;
}

let unscan: (() => void) | null = null;
let uncleanc: (() => void) | null = null;
let procTimer: ReturnType<typeof setInterval> | null = null;

export const useStore = create<State>((set, get) => ({
  view: "dashboard",
  report: null,
  loading: false,
  scanLabel: "",
  process: null,
  settings: null,
  cleanPreview: null,
  cleanBusy: false,
  cleanPhase: null,
  cleanError: null,

  setView: (v) => set({ view: v }),

  refresh: async () => {
    set({ loading: true, scanLabel: "正在初始化…", cleanError: null });
    if (!unscan) {
      onScanProgress((p: ScanProgress) => set({ scanLabel: `扫描 ${p.label}…` }))
        .then((u) => (unscan = u));
    }
    try {
      const report = await api.scan();
      set({ report, loading: false, scanLabel: "" });
    } catch (e) {
      set({ loading: false, scanLabel: "", cleanError: String(e) });
    }
    void get().refreshProcess();
  },

  refreshProcess: async () => {
    try {
      const p = await api.checkProcess();
      set({ process: p });
    } catch {
      /* ignore */
    }
  },

  loadSettings: async () => {
    try {
      set({ settings: await api.getSettings() });
    } catch {
      /* ignore */
    }
  },

  saveSettings: async (s) => {
    try {
      const saved = await api.setSettings(s);
      set({ settings: saved });
    } catch (e) {
      set({ cleanError: String(e) });
    }
  },

  setCleanPreview: (p) => set({ cleanPreview: p }),

  runClean: async (selection) => {
    set({ cleanBusy: true, cleanPhase: null, cleanError: null });
    if (!uncleanc) {
      onCleanProgress((p) => set({ cleanPhase: p }))
        .then((u) => (uncleanc = u));
    }
    try {
      await api.execute(selection);
      set({ cleanBusy: false, cleanPhase: null, cleanPreview: null });
      await get().refresh();
    } catch (e) {
      set({ cleanBusy: false, cleanPhase: null, cleanError: String(e) });
    }
  },
}));

// poll process status
export function startProcessPolling() {
  if (procTimer) return;
  const tick = () => useStore.getState().refreshProcess();
  tick();
  procTimer = setInterval(tick, 3000);
}
