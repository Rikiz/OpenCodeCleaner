import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, onMigrateProgress } from "../lib/invoke";
import { formatBytes } from "../lib/format";
import type { MigrationReport, Preflight, StorageStatus } from "../lib/types";
import * as I from "./icons";

type Stage = "pick" | "preflight" | "running" | "done";

const PHASES: Record<string, string> = {
  checkpoint: "整理数据库",
  copy: "复制文件",
  verify: "校验完整性",
  swap: "切换目录联接",
};

// Ordered so the bar only ever moves forward: one checkpoint pass, then
// copy/verify/swap per root (data, then cache).
const STEPS = [
  "checkpoint:data",
  "copy:data",
  "verify:data",
  "swap:data",
  "copy:cache",
  "verify:cache",
  "swap:cache",
];

export function MigrateWizard({
  status,
  onClose,
  onChanged,
}: {
  status: StorageStatus;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [stage, setStage] = useState<Stage>("pick");
  const [target, setTarget] = useState("");
  const [pf, setPf] = useState<Preflight | null>(null);
  const [report, setReport] = useState<MigrationReport | null>(null);
  const [prog, setProg] = useState<{
    phase: string;
    item: string;
    current: number;
    total: number;
    bytes: number;
  } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const unlisten = useRef<(() => void) | null>(null);

  useEffect(() => {
    onMigrateProgress((p) => setProg(p)).then((f) => (unlisten.current = f));
    return () => unlisten.current?.();
  }, []);

  const pick = async () => {
    setErr(null);
    try {
      const dir = await open({ directory: true, multiple: false, title: "选择目标磁盘上的目录" });
      if (typeof dir === "string") {
        setTarget(dir);
        setStage("preflight");
        const r = await api.migratePreflight(dir);
        setPf(r);
      }
    } catch (e) {
      setErr(String(e));
    }
  };

  const run = async () => {
    setStage("running");
    setErr(null);
    setProg({ phase: "checkpoint", item: "data", current: 0, total: 1, bytes: 0 });
    try {
      const r = await api.migrateExecute(target);
      setReport(r);
      setStage("done");
      onChanged();
    } catch (e) {
      setErr(String(e));
      setStage("preflight");
    }
  };

  const undo = async () => {
    setErr(null);
    setBusy(true);
    try {
      await api.migrateUndo();
      onChanged();
      onClose();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const discard = async () => {
    setErr(null);
    setBusy(true);
    try {
      await api.migrateDiscardBackup();
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const pct = (() => {
    if (!prog) return 0;
    const idx = STEPS.indexOf(`${prog.phase}:${prog.item}`);
    const at = idx < 0 ? 0 : idx;
    const frac = prog.total > 0 ? Math.min(1, prog.current / prog.total) : 0;
    return Math.round(((at + frac) / STEPS.length) * 100);
  })();

  return (
    <div className="overlay" onClick={stage === "running" ? undefined : onClose}>
      <div className="modal bezel" onClick={(e) => e.stopPropagation()}>
        <div className="bezel-inner" style={{ padding: 24 }}>
          <div className="row-between" style={{ marginBottom: 18 }}>
            <div className="h2">
              {stage === "done" ? "迁移完成" : stage === "running" ? "正在迁移…" : "迁移到其它磁盘"}
            </div>
            {stage !== "running" && (
              <button className="btn-icon" onClick={onClose} aria-label="关闭">
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

          {/* ---------- step 1: pick target ---------- */}
          {stage === "pick" && (
            <>
              <div className="muted" style={{ fontSize: 13, lineHeight: 1.7, marginBottom: 16 }}>
                将把 opencode 的会话、日志、快照与缓存整体搬到目标磁盘，
                并在原路径留下 NTFS 目录联接。opencode 无需任何配置或重启即可继续使用。
              </div>
              <div className="preview-item" style={{ marginBottom: 12 }}>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="field-label">当前数据目录</div>
                  <div className="mono dim" style={{ fontSize: 11, wordBreak: "break-all" }}>
                    {status.data_root}
                  </div>
                </div>
                <div style={{ textAlign: "right" }}>
                  <div style={{ fontSize: 15, fontWeight: 600 }}>{formatBytes(status.data_bytes)}</div>
                  <div className="dim" style={{ fontSize: 11 }}>会话与日志</div>
                </div>
              </div>
              <div className="preview-item" style={{ marginBottom: 18 }}>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="field-label">当前缓存目录</div>
                  <div className="mono dim" style={{ fontSize: 11, wordBreak: "break-all" }}>
                    {status.cache_root}
                  </div>
                </div>
                <div style={{ textAlign: "right" }}>
                  <div style={{ fontSize: 15, fontWeight: 600 }}>{formatBytes(status.cache_bytes)}</div>
                  <div className="dim" style={{ fontSize: 11 }}>模型包与二进制</div>
                </div>
              </div>
              <div className="dim" style={{ fontSize: 11.5, marginBottom: 16, lineHeight: 1.6 }}>
                运行时状态目录 <span className="mono">{status.state_dir}</span> 不会被移动（通常很小）。
                迁移过程中请保持 opencode 完全退出。
              </div>
              <button className="btn btn-primary" style={{ width: "100%" }} onClick={pick}>
                选择目标目录…
              </button>
            </>
          )}

          {/* ---------- step 2: preflight ---------- */}
          {stage === "preflight" && pf && (
            <>
              <div className="mono dim" style={{ fontSize: 11, marginBottom: 14, wordBreak: "break-all" }}>
                目标：{target}
              </div>

              <div className="preview-item" style={{ marginBottom: 10 }}>
                <div style={{ flex: 1 }}>待迁移数据</div>
                <div style={{ fontWeight: 600 }}>{formatBytes(pf.data_bytes + pf.cache_bytes)}</div>
              </div>
              <div className="preview-item" style={{ marginBottom: 10 }}>
                <div style={{ flex: 1 }}>目标磁盘可用</div>
                <div style={{ fontWeight: 600 }}>
                  {pf.free_bytes > 0 ? formatBytes(pf.free_bytes) : "未知"}
                </div>
              </div>
              <div className="preview-item" style={{ marginBottom: 14 }}>
                <div style={{ flex: 1 }}>目录联接支持</div>
                <div className="chip" style={{ background: pf.junction_supported ? "rgba(52,211,153,0.12)" : "rgba(251,113,133,0.12)" }}>
                  {pf.junction_supported ? "可用" : "不可用"}
                </div>
              </div>

              {pf.warnings.map((w, i) => (
                <div className="warning-box" key={i} style={{ marginBottom: 10 }}>
                  <I.IconAlert width={15} height={15} />
                  <span>{w}</span>
                </div>
              ))}
              {pf.blockers.map((b, i) => (
                <div className="warning-box" key={i} style={{ marginBottom: 10 }}>
                  <I.IconAlert width={15} height={15} />
                  <span>{b}</span>
                </div>
              ))}

              <div className="row" style={{ gap: 10, marginTop: 6 }}>
                <button className="btn btn-ghost" onClick={() => setStage("pick")} disabled={busy}>
                  重选
                </button>
                <button
                  className="btn btn-primary"
                  style={{ flex: 1 }}
                  onClick={run}
                  disabled={!pf.ok || busy}
                >
                  开始迁移
                </button>
              </div>
              {!pf.ok && (
                <div className="dim" style={{ fontSize: 11.5, marginTop: 10 }}>
                  解决上述问题后才能继续。
                </div>
              )}
            </>
          )}

          {/* ---------- step 3: running ---------- */}
          {stage === "running" && prog && (
            <div style={{ padding: "18px 0" }}>
              <div className="row-between" style={{ marginBottom: 10 }}>
                <span className="muted" style={{ fontSize: 12.5 }}>
                  {PHASES[prog.phase] ?? prog.phase}
                  {prog.phase === "copy" ? `（${prog.item === "data" ? "会话与日志" : "缓存"}）` : ""}
                </span>
                <span className="mono dim" style={{ fontSize: 11.5 }}>
                  {formatBytes(prog.bytes)}
                </span>
              </div>
              <div className="progress-track">
                <div
                  className="progress-fill"
                  style={{
                    width: `${pct}%`,
                  }}
                />
              </div>
              <div className="dim" style={{ fontSize: 11.5, marginTop: 12, lineHeight: 1.6 }}>
                请勿关闭应用。复制完成后会自动校验文件数、字节数与数据库会话数，
                校验通过才会切换目录联接；任何一步失败都会把原目录恢复原位。
              </div>
            </div>
          )}

          {/* ---------- step 4: done ---------- */}
          {stage === "done" && report && (
            <>
              <div className="preview-item" style={{ marginBottom: 10 }}>
                <I.IconCheck width={16} height={16} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 13 }}>{report.items.length} 个目录已迁移</div>
                  <div className="dim" style={{ fontSize: 11 }}>
                    用时 {(report.duration_ms / 1000).toFixed(1)}s
                  </div>
                </div>
              </div>

              {report.items.map((it) => (
                <div className="preview-item" key={it.key} style={{ marginBottom: 9 }}>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 12.5 }}>{it.label}</div>
                    <div className="mono dim" style={{ fontSize: 10.5, wordBreak: "break-all" }}>
                      → {it.target_path}
                    </div>
                  </div>
                  <div style={{ fontSize: 12 }}>{formatBytes(it.copied_bytes)}</div>
                </div>
              ))}

              <div className="warning-box" style={{ marginTop: 14 }}>
                <I.IconAlert width={15} height={15} />
                <span>
                  旧副本仍保留在原磁盘，占 {formatBytes(report.pending_backup_bytes)}。
                  确认新位置一切正常后，再点下面的按钮释放这部分空间——该操作不可撤销。
                </span>
              </div>

              {report.warnings.map((w, i) => (
                <div className="dim" style={{ fontSize: 11, marginTop: 8 }} key={i}>
                  {w}
                </div>
              ))}

              <div className="row" style={{ gap: 10, marginTop: 16 }}>
                <button className="btn btn-ghost" onClick={onClose}>
                  完成
                </button>
                <button className="btn btn-ghost" onClick={undo} disabled={busy}>
                  撤销迁移
                </button>
                <button className="btn btn-primary" style={{ flex: 1 }} onClick={discard} disabled={busy}>
                  删除旧副本（{formatBytes(report.pending_backup_bytes)}）
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}