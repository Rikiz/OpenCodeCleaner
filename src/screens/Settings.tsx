import { useEffect, useState } from "react";
import { api } from "../lib/invoke";
import { formatBytes } from "../lib/format";
import * as I from "../components/icons";
import { MigrateWizard } from "../components/MigrateWizard";
import type { AppSettings as S, StorageStatus } from "../lib/types";

const DEFAULTS: S = {
  language: "zh",
  default_keep_recent: 50,
  backup_retention: 5,
  auto_update_check: true,
  theme: "dark",
};

export function Settings() {
  const [s, setS] = useState<S | null>(null);
  const [saved, setSaved] = useState(false);
  const [dataRoot, setDataRoot] = useState("");
  const [diag, setDiag] = useState<string | null>(null);
  const [storage, setStorage] = useState<StorageStatus | null>(null);
  const [migrating, setMigrating] = useState(false);
  const [busy, setBusy] = useState(false);
  const [relErr, setRelErr] = useState<string | null>(null);

  const loadStorage = () => {
    api.storageInfo().then(setStorage).catch(() => {});
  };

  useEffect(() => {
    api.getSettings().then((v) => setS({ ...DEFAULTS, ...v }));
    api.getDataRoot().then(setDataRoot).catch(() => {});
    loadStorage();
  }, []);

  const update = (patch: Partial<S>) => {
    if (!s) return;
    const next = { ...s, ...patch };
    setS(next);
  };

  const save = async () => {
    if (!s) return;
    try {
      await api.setSettings(s);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch {
      /* ignore */
    }
  };

  const exportDiag = async () => {
    try {
      const p = await api.exportDiagnostics();
      setDiag(p);
      setTimeout(() => setDiag(null), 4000);
    } catch {
      /* ignore */
    }
  };

  const undoMigration = async () => {
    setRelErr(null);
    setBusy(true);
    try {
      await api.migrateUndo();
      loadStorage();
    } catch (e) {
      setRelErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const discardOld = async () => {
    setRelErr(null);
    setBusy(true);
    try {
      await api.migrateDiscardBackup();
      loadStorage();
    } catch (e) {
      setRelErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!s) return <div className="loading"><div className="spinner" /></div>;

  return (
    <div className="enter" style={{ maxWidth: 680 }}>
      <div className="section-head">
        <div>
          <div className="eyebrow">偏好设置</div>
          <h1 className="h1" style={{ marginTop: 8 }}>设置</h1>
        </div>
        <div className="row">
          {saved && <span className="chip accent">已保存</span>}
          <button className="btn btn-primary btn-sm" onClick={save}>
            <I.IconCheck width={14} height={14} /> 保存
          </button>
        </div>
      </div>

      <div className="bezel section">
        <div className="bezel-inner">
          <div className="h3" style={{ marginBottom: 4 }}>清理策略</div>

          <div className="field">
            <div>
              <div className="field-label">保留最近 N 个会话</div>
              <div className="field-hint">一键瘦身时使用此保留策略</div>
            </div>
            <input
              className="input"
              type="number"
              min={0}
              value={s.default_keep_recent}
              onChange={(e) => update({ default_keep_recent: Number(e.target.value) })}
            />
          </div>

          <div className="field">
            <div>
              <div className="field-label">备份保留份数</div>
              <div className="field-hint">超出自动删除最旧的备份</div>
            </div>
            <input
              className="input"
              type="number"
              min={1}
              value={s.backup_retention}
              onChange={(e) => update({ backup_retention: Number(e.target.value) })}
            />
          </div>

          <div className="field">
            <div>
              <div className="field-label">语言</div>
              <div className="field-hint">界面语言</div>
            </div>
            <select
              className="select"
              value={s.language}
              onChange={(e) => update({ language: e.target.value })}
            >
              <option value="zh">简体中文</option>
              <option value="en">English</option>
            </select>
          </div>

          <div className="field">
            <div>
              <div className="field-label">主题</div>
              <div className="field-hint">v1 仅暗色</div>
            </div>
            <select
              className="select"
              value={s.theme}
              onChange={(e) => update({ theme: e.target.value })}
            >
              <option value="dark">暗色</option>
              <option value="system">跟随系统</option>
            </select>
          </div>

          <div className="field">
            <div>
              <div className="field-label">自动检查更新</div>
              <div className="field-hint">启动时检查应用更新</div>
            </div>
            <button
              className={"toggle" + (s.auto_update_check ? " on" : "")}
              onClick={() => update({ auto_update_check: !s.auto_update_check })}
            />
          </div>
        </div>
      </div>

      <div className="bezel section">
        <div className="bezel-inner">
          <div className="h3" style={{ marginBottom: 8 }}>数据与诊断</div>
          <div className="field">
            <div>
              <div className="field-label">opencode 数据目录</div>
              <div className="field-hint mono">{dataRoot || "—"}</div>
            </div>
            <I.IconDatabase width={18} height={18} />
          </div>
          <div className="field">
            <div>
              <div className="field-label">导出诊断包</div>
              <div className="field-hint">包含应用日志，用于排查问题（不含会话内容）</div>
              {diag && <div className="chip accent" style={{ marginTop: 6 }}>{formatBytes(0)} 已生成</div>}
            </div>
            <button className="btn btn-ghost btn-sm" onClick={exportDiag}>
              导出
            </button>
          </div>
        </div>
      </div>

      {storage?.supported && (
        <div className="bezel section">
          <div className="bezel-inner">
            <div className="h3" style={{ marginBottom: 8 }}>数据迁移</div>

            {relErr && (
              <div className="warning-box" style={{ marginBottom: 12 }}>
                <I.IconAlert width={15} height={15} />
                <span>{relErr}</span>
              </div>
            )}

            {!storage.data_relocated && !storage.cache_relocated ? (
              <>
                <div className="field-hint" style={{ marginBottom: 14, lineHeight: 1.7 }}>
                  opencode 的数据默认落在系统盘。把会话、日志、快照与缓存整体搬到其它磁盘，
                  并在原路径留下目录联接——opencode 无需配置或重启。
                </div>
                <div className="field">
                  <div>
                    <div className="field-label">当前占用</div>
                    <div className="field-hint mono">
                      {formatBytes(storage.data_bytes)} 数据 · {formatBytes(storage.cache_bytes)} 缓存
                    </div>
                  </div>
                  <button
                    className="btn btn-primary btn-sm"
                    onClick={() => setMigrating(true)}
                    disabled={storage.opencode_running}
                  >
                    迁移到其它磁盘
                  </button>
                </div>
                {storage.opencode_running && (
                  <div className="dim" style={{ fontSize: 11.5, marginTop: 10 }}>
                    opencode 正在运行，迁移前请完全退出。
                  </div>
                )}
              </>
            ) : (
              <>
                <div className="field">
                  <div>
                    <div className="field-label">已迁移</div>
                    <div className="field-hint mono" style={{ wordBreak: "break-all" }}>
                      数据 → {storage.data_real_path}
                      <br />
                      缓存 → {storage.cache_real_path}
                    </div>
                  </div>
                  <div className="chip" style={{ background: "rgba(52,211,153,0.12)" }}>
                    联接生效中
                  </div>
                </div>
                {storage.pending_backup_bytes > 0 && (
                  <div className="warning-box" style={{ margin: "12px 0" }}>
                    <I.IconAlert width={15} height={15} />
                    <span>
                      原磁盘上还留着迁移时的旧副本，占 {formatBytes(storage.pending_backup_bytes)}。
                      确认新位置一切正常后再删除，此操作不可撤销。
                    </span>
                  </div>
                )}
                <div className="row" style={{ gap: 10, marginTop: 12 }}>
                  <button className="btn btn-ghost btn-sm" onClick={undoMigration} disabled={busy}>
                    撤销迁移
                  </button>
                  {storage.pending_backup_bytes > 0 && (
                    <button className="btn btn-sm" onClick={discardOld} disabled={busy}>
                      删除旧副本（{formatBytes(storage.pending_backup_bytes)}）
                    </button>
                  )}
                </div>
              </>
            )}
          </div>
        </div>
      )}

      <div className="dim" style={{ fontSize: 11.5, lineHeight: 1.6, padding: "0 4px" }}>
        opencode cleaner · 本地运行，不联网，不采集任何数据。
      </div>

      {migrating && storage && (
        <MigrateWizard
          status={storage}
          onClose={() => setMigrating(false)}
          onChanged={loadStorage}
        />
      )}
    </div>
  );
}
