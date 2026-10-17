import { useEffect, useState } from "react";
import { api } from "../lib/invoke";
import { formatBytes } from "../lib/format";
import type { CleanPreview, CleanResult, CleanSelection } from "../lib/types";
import { useStore } from "../store";
import * as I from "./icons";

type Stage = "preview" | "confirm" | "running" | "done";

const PHASES: { key: string; label: string }[] = [
  { key: "backup", label: "创建备份" },
  { key: "deleting", label: "删除会话数据" },
  { key: "vacuum", label: "压缩数据库" },
  { key: "files", label: "清理目录" },
];

export function CleanWizard({
  selection,
  onClose,
}: {
  selection: CleanSelection | null;
  onClose: () => void;
}) {
  const refresh = useStore((s) => s.refresh);
  const phase = useStore((s) => s.cleanPhase);
  const [stage, setStage] = useState<Stage>("preview");
  const [preview, setPreview] = useState<CleanPreview | null>(null);
  const [result, setResult] = useState<CleanResult | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!selection) {
      setStage("preview");
      setPreview(null);
      setResult(null);
      setErr(null);
      return;
    }
    setLoading(true);
    setErr(null);
    api
      .dryRun(selection)
      .then((p) => {
        setPreview(p);
        setStage("confirm");
      })
      .catch((e) => setErr(String(e)))
      .finally(() => setLoading(false));
  }, [selection]);

  if (!selection) return null;

  const close = () => {
    onClose();
  };

  const run = async () => {
    setStage("running");
    setErr(null);
    try {
      const res = await api.execute(selection);
      setResult(res);
      setStage("done");
      await refresh();
    } catch (e) {
      setErr(String(e));
      setStage("confirm");
    }
  };

  const phasesDone = (() => {
    if (!phase) return new Set<string>();
    return new Set<string>();
  })();

  return (
    <div className="overlay" onClick={stage === "running" ? undefined : close}>
      <div className="modal bezel" onClick={(e) => e.stopPropagation()}>
        <div className="bezel-inner">
          <div className="row-between" style={{ marginBottom: 20 }}>
            <div>
              <div className="eyebrow">清理预览</div>
              <div className="h2" style={{ marginTop: 6 }}>
                {stage === "done" ? "清理完成" : stage === "running" ? "正在清理…" : "确认清理"}
              </div>
            </div>
            {stage !== "running" && (
              <button className="btn-icon" onClick={close} aria-label="关闭">
                <I.IconClose width={16} height={16} />
              </button>
            )}
          </div>

          {err && (
            <div className="warning-box" style={{ marginBottom: 16 }}>
              <I.IconAlert width={15} height={15} />
              <span>{err}</span>
            </div>
          )}

          {loading && (
            <div className="loading" style={{ padding: 40 }}>
              <div className="spinner" />
              <div>计算可回收空间…</div>
            </div>
          )}

          {stage === "confirm" && preview && (
            <>
              <div className="bezel" style={{ marginBottom: 16, padding: 6 }}>
                <div className="bezel-inner" style={{ padding: 18 }}>
                  <div className="row-between" style={{ marginBottom: 14 }}>
                    <span className="muted" style={{ fontSize: 12.5 }}>
                      预计回收
                    </span>
                  </div>
                  <div style={{ fontSize: 38, fontWeight: 600, letterSpacing: "-0.03em" }}>
                    {formatBytes(preview.total_freed_bytes)}
                  </div>
                  <div className="dim" style={{ fontSize: 12, marginTop: 6 }}>
                    {preview.affected_session_count} 个会话 · 备份约{" "}
                    {formatBytes(preview.backup_estimated_bytes)}
                  </div>
                </div>
              </div>

              <div style={{ maxHeight: 200, overflowY: "auto", marginBottom: 14 }}>
                {preview.items.map((it, i) => (
                  <div className="preview-item" key={i}>
                    <I.IconTrash width={15} height={15} />
                    <div style={{ flex: 1 }}>
                      <div className="mono" style={{ color: "var(--fg-0)" }}>
                        {it.label}
                      </div>
                    </div>
                    <div style={{ color: "var(--fg-1)", fontVariantNumeric: "tabular-nums" }}>
                      {formatBytes(it.size_bytes)}
                    </div>
                  </div>
                ))}
              </div>

              {preview.warnings.map((w, i) => (
                <div className="warning-box" key={i}>
                  <I.IconAlert width={14} height={14} />
                  <span>{w}</span>
                </div>
              ))}

              <div className="row-between" style={{ marginTop: 20 }}>
                <span className="dim" style={{ fontSize: 12 }}>
                  将自动备份，可在「备份」页恢复
                </span>
                <div className="row">
                  <button className="btn btn-ghost btn-sm" onClick={close}>
                    取消
                  </button>
                  <button className="btn btn-danger btn-sm" onClick={run}>
                    <I.IconBroom width={15} height={15} />
                    确认清理
                  </button>
                </div>
              </div>
            </>
          )}

          {stage === "running" && (
            <div style={{ padding: "10px 0" }}>
              <div className="phase-list">
                {PHASES.map((p) => {
                  const active = phase?.phase === p.key;
                  const done = phasesDone.has(p.key);
                  return (
                    <div
                      key={p.key}
                      className={"phase" + (active ? " active" : done ? " done" : "")}
                    >
                      <span className="phase-icon">
                        {active ? (
                          <span className="spinner" style={{ width: 8, height: 8 }} />
                        ) : done ? (
                          <I.IconCheck width={11} height={11} />
                        ) : null}
                      </span>
                      {p.label}
                      {active && phase && phase.total > 0 && (
                        <span className="dim" style={{ marginLeft: "auto", fontSize: 11.5 }}>
                          {phase.current}/{phase.total}
                        </span>
                      )}
                    </div>
                  );
                })}
              </div>
              {phase && phase.total > 0 && (
                <div className="progress-track" style={{ marginTop: 22 }}>
                  <div
                    className="progress-fill"
                    style={{
                      width: `${Math.round((phase.current / Math.max(phase.total, 1)) * 100)}%`,
                    }}
                  />
                </div>
              )}
            </div>
          )}

          {stage === "done" && result && (
            <div style={{ textAlign: "center", padding: "16px 0" }}>
              <div
                style={{
                  width: 56,
                  height: 56,
                  borderRadius: "50%",
                  margin: "0 auto 18px",
                  display: "grid",
                  placeItems: "center",
                  background: "rgba(52,211,153,0.12)",
                  border: "1px solid rgba(52,211,153,0.25)",
                  color: "var(--safe)",
                }}
              >
                <I.IconCheck width={26} height={26} />
              </div>
              <div className="result-amount">{formatBytes(result.freed_bytes)}</div>
              <div className="muted" style={{ marginTop: 8, fontSize: 13 }}>
                已回收 · 用时 {(result.duration_ms / 1000).toFixed(1)}s
              </div>
              {result.errors.length > 0 && (
                <div
                  style={{
                    marginTop: 16,
                    padding: "10px 12px",
                    borderRadius: 8,
                    textAlign: "left",
                    background: "rgba(251, 191, 36, 0.08)",
                    border: "1px solid rgba(251, 191, 36, 0.25)",
                  }}
                >
                  <div style={{ fontSize: 12, color: "var(--warn)" }}>
                    {result.errors.length} 项未能清理
                  </div>
                  <ul
                    style={{
                      margin: "6px 0 0",
                      paddingLeft: 18,
                      fontSize: 11,
                      lineHeight: 1.5,
                      wordBreak: "break-all",
                    }}
                  >
                    {result.errors.slice(0, 8).map((e, i) => (
                      <li key={i}>
                        <span className="mono">{e.item}</span>: {e.message}
                      </li>
                    ))}
                    {result.errors.length > 8 && (
                      <li className="dim">…其余 {result.errors.length - 8} 项省略</li>
                    )}
                  </ul>
                  <div className="dim" style={{ fontSize: 11, marginTop: 6 }}>
                    常见原因：opencode 仍在运行并占用文件（Windows 下会锁定句柄）。
                  </div>
                </div>
              )}
              {result.backup_path && (
                <div className="dim mono" style={{ marginTop: 16, fontSize: 11, wordBreak: "break-all" }}>
                  已备份至 {result.backup_path}
                </div>
              )}
              <button className="btn btn-primary" style={{ marginTop: 24 }} onClick={close}>
                完成
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
