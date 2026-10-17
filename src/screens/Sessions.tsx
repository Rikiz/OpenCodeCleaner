import { useMemo, useState } from "react";
import { useStore } from "../store";
import { formatBytes, relativeTime } from "../lib/format";
import { SafetyBar } from "../components/SafetyBar";
import * as I from "../components/icons";
import type { CleanSelection } from "../lib/types";

export function Sessions({
  onClean,
}: {
  onClean: (sel: CleanSelection) => void;
}) {
  const report = useStore((s) => s.report);
  const process = useStore((s) => s.process);

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [project, setProject] = useState("");

  const projects = useMemo(() => {
    const m = new Map<string, string>();
    report?.sessions.forEach((s) => {
      if (s.project_name) m.set(s.project_id, s.project_name);
    });
    return Array.from(m.entries());
  }, [report]);

  const filtered = useMemo(() => {
    if (!report) return [];
    return report.sessions.filter((s) => {
      if (project && s.project_id !== project) return false;
      if (filter && !s.title.toLowerCase().includes(filter.toLowerCase())) return false;
      return true;
    });
  }, [report, filter, project]);

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  };

  const allChecked = filtered.length > 0 && filtered.every((s) => selected.has(s.id));
  const toggleAll = () => {
    if (allChecked) setSelected(new Set());
    else setSelected(new Set(filtered.map((s) => s.id)));
  };

  const cleanSelected = () => {
    onClean({
      session_ids: Array.from(selected),
      keep_recent: null,
      older_than_days: null,
      min_size_mb: null,
      project_ids: [],
      directories: [],
    });
    setSelected(new Set());
  };

  const cleanOld = () => {
    onClean({
      session_ids: [],
      keep_recent: useStore.getState().settings?.default_keep_recent ?? 50,
      older_than_days: null,
      min_size_mb: null,
      project_ids: [],
      directories: [],
    });
  };

  if (!report) return <div className="empty">请先扫描</div>;

  const selectedBytes = report.sessions
    .filter((s) => selected.has(s.id))
    .reduce((a, s) => a + s.size_bytes, 0);

  return (
    <div className="enter">
      <SafetyBar />

      <div className="section-head">
        <div>
          <div className="eyebrow">会话管理</div>
          <h1 className="h1" style={{ marginTop: 8 }}>
            历史会话
          </h1>
          <div className="muted" style={{ marginTop: 6, fontSize: 13 }}>
            {report.sessions.length} 个会话 · 总计 {formatBytes(report.session_total_bytes)}
          </div>
        </div>
        <div className="row">
          <button className="btn btn-ghost btn-sm" onClick={cleanOld}>
            <I.IconArchive width={14} height={14} /> 保留最近 50 个
          </button>
          <button
            className="btn btn-danger btn-sm"
            disabled={selected.size === 0 || process?.running}
            onClick={cleanSelected}
            title={process?.running ? "请先退出 opencode" : ""}
          >
            <I.IconTrash width={14} height={14} /> 删除所选 ({selected.size})
          </button>
        </div>
      </div>

      <div className="bezel section">
        <div className="bezel-inner">
          <div className="row" style={{ marginBottom: 16 }}>
            <input
              className="input"
              style={{ width: 220 }}
              placeholder="搜索会话标题…"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            <select
              className="select"
              value={project}
              onChange={(e) => setProject(e.target.value)}
            >
              <option value="">全部项目</option>
              {projects.map(([id, name]) => (
                <option key={id} value={id}>
                  {name}
                </option>
              ))}
            </select>
            {selected.size > 0 && (
              <span className="chip accent">{formatBytes(selectedBytes)} 已选</span>
            )}
          </div>

          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 40 }}>
                  <input
                    type="checkbox"
                    className="check"
                    checked={allChecked}
                    onChange={toggleAll}
                  />
                </th>
                <th>会话</th>
                <th>项目</th>
                <th>模型</th>
                <th>活跃</th>
                <th style={{ textAlign: "right" }}>占用</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((s) => (
                <tr
                  key={s.id}
                  onClick={() => toggle(s.id)}
                  style={{ cursor: "pointer" }}
                >
                  <td onClick={(e) => e.stopPropagation()}>
                    <input
                      type="checkbox"
                      className="check"
                      checked={selected.has(s.id)}
                      onChange={() => toggle(s.id)}
                    />
                  </td>
                  <td>
                    <div className="row-title">{s.title || "(未命名)"}</div>
                    <div className="dim mono" style={{ marginTop: 2 }}>
                      {s.id.slice(0, 22)}…
                    </div>
                  </td>
                  <td>{s.project_name || <span className="dim">—</span>}</td>
                  <td>
                    {s.model_id ? (
                      <span className="chip">{s.model_id}</span>
                    ) : (
                      <span className="dim">—</span>
                    )}
                  </td>
                  <td className="dim" style={{ whiteSpace: "nowrap" }}>
                    {relativeTime(s.last_active)}
                  </td>
                  <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>
                    {formatBytes(s.size_bytes)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          {filtered.length === 0 && <div className="empty">无匹配会话</div>}
        </div>
      </div>
    </div>
  );
}
