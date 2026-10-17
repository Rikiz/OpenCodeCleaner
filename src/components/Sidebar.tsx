import { useStore } from "../store";
import * as I from "./icons";
import type { View } from "../lib/types";

const items: { id: View; label: string; icon: (p: any) => JSX.Element }[] = [
  { id: "dashboard", label: "总览", icon: I.IconDashboard },
  { id: "sessions", label: "会话", icon: I.IconChat },
  { id: "directories", label: "目录", icon: I.IconFolder },
  { id: "backups", label: "备份", icon: I.IconArchive },
  { id: "settings", label: "设置", icon: I.IconSettings },
];

export function Sidebar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const report = useStore((s) => s.report);

  const totalMB = report ? report.total_mb.toFixed(0) : "—";

  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-mark">
          <I.IconBroom width={16} height={16} stroke="#fff" />
        </div>
        <div>
          <div className="brand-name">cleaner</div>
          <div className="brand-sub">opencode · {totalMB} MB</div>
        </div>
      </div>

      <nav className="nav">
        {items.map((it) => {
          const Icon = it.icon;
          return (
            <button
              key={it.id}
              className={"nav-item" + (view === it.id ? " active" : "")}
              onClick={() => setView(it.id)}
            >
              <Icon width={17} height={17} />
              {it.label}
            </button>
          );
        })}
      </nav>

      <div className="nav-spacer" />

      <div className="dim" style={{ fontSize: 11, padding: "0 10px", lineHeight: 1.5 }}>
        本地运行 · 不联网
      </div>
    </aside>
  );
}
