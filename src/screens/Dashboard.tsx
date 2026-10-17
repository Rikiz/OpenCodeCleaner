import { useStore } from "../store";
import { formatBytes, dirColor, dirLabel, DIR_ORDER, relativeTime } from "../lib/format";
import { DonutChart } from "../components/DonutChart";
import { SafetyBar } from "../components/SafetyBar";
import * as I from "../components/icons";
import type { CleanSelection } from "../lib/types";

export function Dashboard({
  onClean,
}: {
  onClean: (sel: CleanSelection) => void;
}) {
  const report = useStore((s) => s.report);
  const loading = useStore((s) => s.loading);
  const scanLabel = useStore((s) => s.scanLabel);
  const refresh = useStore((s) => s.refresh);

  if (loading && !report) {
    return (
      <div className="loading">
        <div className="spinner" />
        <div>{scanLabel || "扫描中…"}</div>
      </div>
    );
  }
  if (!report) {
    return (
      <div className="empty">
        <div style={{ marginBottom: 16 }}>未找到 opencode 数据</div>
        <button className="btn btn-ghost" onClick={refresh}>
          <I.IconRefresh width={15} height={15} /> 重新扫描
        </button>
      </div>
    );
  }

  const slices = DIR_ORDER.map((k) => {
    const d = report.directories.find((x) => x.key === k);
    return { key: k, value: d?.size_bytes ?? 0, color: dirColor(k), label: dirLabel(k) };
  }).filter((s) => s.value > 0);

  const topSessions = [...report.sessions].sort((a, b) => b.size_bytes - a.size_bytes).slice(0, 6);
  const maxSession = topSessions[0]?.size_bytes ?? 1;
  const dbBytes = report.total_bytes - report.directories.reduce((s, d) => s + d.size_bytes, 0);

  // default quick clean: keep recent 50 sessions + all directories
  const quickClean = () => {
    const sel: CleanSelection = {
      session_ids: [],
      keep_recent: useStore.getState().settings?.default_keep_recent ?? 50,
      older_than_days: null,
      min_size_mb: null,
      project_ids: [],
      directories: DIR_ORDER,
    };
    onClean(sel);
  };

  return (
    <div className="enter enter-stagger">
      <SafetyBar />

      <div className="section-head">
        <div>
          <div className="eyebrow">磁盘占用总览</div>
          <h1 className="h1" style={{ marginTop: 8 }}>
            {formatBytes(report.total_bytes)}
          </h1>
          <div className="muted" style={{ marginTop: 6, fontSize: 13 }}>
            {report.opencode_version ? `opencode ${report.opencode_version}` : "opencode"} ·{" "}
            {report.sessions.length} 个会话 · 扫描于 {relativeTime(report.scanned_at)}
          </div>
        </div>
        <div className="row">
          <button className="btn btn-ghost btn-sm" onClick={refresh}>
            <I.IconRefresh width={14} height={14} /> 刷新
          </button>
          <button className="btn btn-primary btn-sm" onClick={quickClean}>
            <I.IconBroom width={14} height={14} /> 一键瘦身
          </button>
        </div>
      </div>

      <div className="grid grid-bento section">
        <div className="bezel">
          <div className="bezel-inner">
            <div className="h3" style={{ marginBottom: 18 }}>
              占用分布
            </div>
            {slices.length > 0 ? (
              <div className="donut-wrap">
                <DonutChart
                  slices={slices}
                  centerValue={formatBytes(report.total_bytes, 0)}
                  centerLabel="总计"
                />
                <div className="legend">
                  <div className="legend-row">
                    <span className="legend-swatch" style={{ background: "#9aa3b2" }} />
                    <span className="legend-name">会话数据库</span>
                    <span className="legend-val">{formatBytes(dbBytes)}</span>
                  </div>
                  {slices.map((s) => (
                    <div className="legend-row" key={s.key}>
                      <span className="legend-swatch" style={{ background: s.color }} />
                      <span className="legend-name">{s.label}</span>
                      <span className="legend-val">{formatBytes(s.value)}</span>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="empty">暂无目录数据</div>
            )}
          </div>
        </div>

        <div className="bezel">
          <div className="bezel-inner">
            <div className="h3" style={{ marginBottom: 12 }}>
              可回收空间
            </div>
            <div className="stat-value">
              {formatBytes(
                report.directories.reduce((s, d) => s + d.size_bytes, 0)
              )}
            </div>
            <div className="stat-sub">
              日志、缓存、快照等非会话数据可一键清理，
              <br />
              且 opencode 运行时也可安全清理。
            </div>
            <button
              className="btn btn-ghost btn-sm"
              style={{ marginTop: 18 }}
              onClick={() =>
                onClean({
                  session_ids: [],
                  keep_recent: null,
                  older_than_days: null,
                  min_size_mb: null,
                  project_ids: [],
                  directories: DIR_ORDER,
                })
              }
            >
              清理非会话数据 <I.IconArrowRight width={13} height={13} />
            </button>
          </div>
        </div>
      </div>

      <div className="section">
        <div className="section-head">
          <div className="h2">最占空间的会话</div>
        </div>
        <div className="bezel">
          <div className="bezel-inner">
            <div className="barlist">
              {topSessions.map((s) => (
                <div className="bar" key={s.id}>
                  <div className="bar-head">
                    <span className="bar-title">{s.title || "(未命名会话)"}</span>
                    <span className="bar-meta">
                      {formatBytes(s.size_bytes)} · {s.part_count} parts · {relativeTime(s.last_active)}
                    </span>
                  </div>
                  <div className="bar-track">
                    <div
                      className="bar-fill"
                      style={{ width: `${(s.size_bytes / maxSession) * 100}%` }}
                    />
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
