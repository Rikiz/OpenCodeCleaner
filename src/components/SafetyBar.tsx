import { useStore } from "../store";
import * as I from "./icons";

export function SafetyBar() {
  const process = useStore((s) => s.process);
  if (!process) return null;

  if (process.running) {
    return (
      <div className="safety warn">
        <span className="dot warn" />
        <I.IconAlert width={15} height={15} />
        <span>
          检测到 opencode 正在运行（{process.processes.join(", ")}）。请先退出 opencode，
          才能清理会话数据库；日志/缓存/快照目录仍可清理。
        </span>
      </div>
    );
  }
  return (
    <div className="safety safe">
      <span className="dot safe" />
      <I.IconShield width={15} height={15} />
      <span>opencode 未运行，可安全清理全部数据。</span>
    </div>
  );
}
