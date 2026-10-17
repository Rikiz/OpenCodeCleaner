import { useEffect, useState } from "react";
import { api } from "../lib/invoke";
import { formatBytes } from "../lib/format";
import * as I from "../components/icons";
import type { AppSettings as S } from "../lib/types";

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

  useEffect(() => {
    api.getSettings().then((v) => setS({ ...DEFAULTS, ...v }));
    api.getDataRoot().then(setDataRoot).catch(() => {});
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

      <div className="dim" style={{ fontSize: 11.5, lineHeight: 1.6, padding: "0 4px" }}>
        opencode cleaner · 本地运行，不联网，不采集任何数据。
      </div>
    </div>
  );
}
