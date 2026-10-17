import { useEffect, useState } from "react";
import { api } from "../lib/invoke";
import { useStore } from "../store";
import { formatBytes, formatTime } from "../lib/format";
import * as I from "../components/icons";
import type { BackupInfo } from "../lib/types";

export function Backups() {
  const refresh = useStore((s) => s.refresh);
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const load = () => {
    api.getBackups().then(setBackups).catch((e) => setErr(String(e)));
  };

  useEffect(load, []);

  const restore = async (id: string) => {
    if (!confirm("恢复将覆盖当前 opencode 数据，确认继续？")) return;
    setBusy(id);
    try {
      await api.restore(id);
      await refresh();
      load();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const del = async (id: string) => {
    if (!confirm("删除此备份？")) return;
    try {
      await api.deleteBackup(id);
      load();
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <div className="enter">
      <div className="section-head">
        <div>
          <div className="eyebrow">备份与恢复</div>
          <h1 className="h1" style={{ marginTop: 8 }}>
            备份记录
          </h1>
          <div className="muted" style={{ marginTop: 6, fontSize: 13 }}>
            每次清理前自动备份，可从此处恢复
          </div>
        </div>
        <button className="btn btn-ghost btn-sm" onClick={load}>
          <I.IconRefresh width={14} height={14} /> 刷新
        </button>
      </div>

      {err && (
        <div className="warning-box" style={{ marginBottom: 16 }}>
          <I.IconAlert width={15} height={15} />
          <span>{err}</span>
        </div>
      )}

      <div className="bezel section">
        <div className="bezel-inner">
          {backups.length === 0 ? (
            <div className="empty">暂无备份</div>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>备份</th>
                  <th>时间</th>
                  <th>大小</th>
                  <th style={{ width: 200, textAlign: "right" }}>操作</th>
                </tr>
              </thead>
              <tbody>
                {backups.map((b) => (
                  <tr key={b.id}>
                    <td>
                      <div className="row-title mono">{b.id}</div>
                      <div className="dim" style={{ fontSize: 11, marginTop: 2 }}>
                        {b.label}
                      </div>
                    </td>
                    <td className="dim">{formatTime(b.created_at)}</td>
                    <td style={{ fontVariantNumeric: "tabular-nums" }}>{formatBytes(b.size_bytes)}</td>
                    <td style={{ textAlign: "right" }}>
                      <div className="row" style={{ justifyContent: "flex-end" }}>
                        <button
                          className="btn btn-ghost btn-sm"
                          disabled={busy === b.id}
                          onClick={() => restore(b.id)}
                        >
                          {busy === b.id ? <span className="spinner" /> : <I.IconArchive width={13} height={13} />}
                          恢复
                        </button>
                        <button className="btn-icon" onClick={() => del(b.id)} aria-label="删除备份">
                          <I.IconTrash width={14} height={14} />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
