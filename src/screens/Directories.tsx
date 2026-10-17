import { useState } from "react";
import { useStore } from "../store";
import { formatBytes, dirColor, dirLabel, DIR_ORDER } from "../lib/format";
import { SafetyBar } from "../components/SafetyBar";
import * as I from "../components/icons";
import type { CleanSelection, DirKey } from "../lib/types";

export function Directories({
  onClean,
}: {
  onClean: (sel: CleanSelection) => void;
}) {
  const report = useStore((s) => s.report);
  const [checked, setChecked] = useState<Set<DirKey>>(new Set());

  if (!report) return <div className="empty">请先扫描</div>;

  const totalSelected = report.directories
    .filter((d) => checked.has(d.key))
    .reduce((a, d) => a + d.size_bytes, 0);

  const toggle = (k: DirKey) => {
    setChecked((prev) => {
      const next = new Set(prev);
      next.has(k) ? next.delete(k) : next.add(k);
      return next;
    });
  };

  const runClean = () => {
    onClean({
      session_ids: [],
      keep_recent: null,
      older_than_days: null,
      min_size_mb: null,
      project_ids: [],
      directories: Array.from(checked),
    });
    setChecked(new Set());
  };

  return (
    <div className="enter">
      <SafetyBar />

      <div className="section-head">
        <div>
          <div className="eyebrow">目录清理</div>
          <h1 className="h1" style={{ marginTop: 8 }}>
            非会话数据
          </h1>
          <div className="muted" style={{ marginTop: 6, fontSize: 13 }}>
            这些数据与具体会话无关，opencode 运行时也可安全清理
          </div>
        </div>
        <button
          className="btn btn-primary btn-sm"
          disabled={checked.size === 0}
          onClick={runClean}
        >
          <I.IconBroom width={14} height={14} /> 清理所选 ({formatBytes(totalSelected)})
        </button>
      </div>

      <div className="grid grid-2 enter-stagger">
        {DIR_ORDER.map((k) => {
          const d = report.directories.find((x) => x.key === k);
          const size = d?.size_bytes ?? 0;
          const on = checked.has(k);
          return (
            <div
              key={k}
              className="card"
              style={{
                cursor: "pointer",
                borderColor: on ? "rgba(124,92,255,0.4)" : undefined,
                background: on ? "rgba(124,92,255,0.06)" : undefined,
              }}
              onClick={() => toggle(k)}
            >
              <div className="row-between" style={{ marginBottom: 14 }}>
                <div className="row">
                  <span
                    style={{
                      width: 32,
                      height: 32,
                      borderRadius: 9,
                      display: "grid",
                      placeItems: "center",
                      background: `${dirColor(k)}22`,
                      color: dirColor(k),
                    }}
                  >
                    <I.IconFolder width={16} height={16} />
                  </span>
                  <div>
                    <div className="h3">{dirLabel(k)}</div>
                    <div className="dim" style={{ fontSize: 11.5, marginTop: 2 }}>
                      {d?.file_count ?? 0} 个文件
                    </div>
                  </div>
                </div>
                <input
                  type="checkbox"
                  className="check"
                  checked={on}
                  onChange={() => toggle(k)}
                  onClick={(e) => e.stopPropagation()}
                />
              </div>
              <div style={{ fontSize: 24, fontWeight: 600, letterSpacing: "-0.03em" }}>
                {formatBytes(size)}
              </div>
              <div className="bar-track" style={{ marginTop: 12 }}>
                <div
                  className="bar-fill"
                  style={{
                    width: `${Math.min(100, (size / Math.max(report.total_bytes, 1)) * 100)}%`,
                    background: `linear-gradient(90deg, ${dirColor(k)}, ${dirColor(k)}aa)`,
                  }}
                />
              </div>
              <div className="dim mono" style={{ fontSize: 10.5, marginTop: 10, wordBreak: "break-all" }}>
                {d?.path}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
