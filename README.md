# opencode cleaner

为 opencode（CLI 与 Desktop）瘦身的跨平台（macOS / Windows）桌面应用。扫描磁盘占用，安全清理历史会话、日志、快照与缓存。

## 功能（v1）

- **占用扫描与可视化**：环形图展示各目录占比 + 会话占用排行，5 秒内完成。
- **历史会话清理**：按时间 / 大小 / 项目 / 关键词筛选，批量删除（含 DB 级联 + event 孤儿清理）；「保留最近 N 个」一键瘦身。
- **目录清理**：日志 / 快照 / session_diff / 缓存等，opencode 运行时也可清理。
- **安全机制**：opencode 运行检测（阻止 DB 操作）→ dry-run 预览精确可回收字节 → 清理前自动备份（DB checkpoint + 目录归档）→ VACUUM 回收空间；备份可一键恢复。
- **本地优先**：不联网、不采集任何数据、不触碰凭证与对话正文。

## 技术栈

- **桌面壳**：Tauri 2（Rust 核心 + 系统 WebView）
- **前端**：React 18 + TypeScript + Vite
- **DB**：`rusqlite`（bundled SQLite）直连 `opencode.db`
- **进程检测**：`sysinfo`

## 项目结构

```
docs/                 # SPEC / ARCHITECTURE / CONTRACTS（SDD 文档）
src-tauri/src/        # Rust 核心引擎 + Tauri 命令
  ├─ paths.rs  db.rs  scan.rs  clean.rs  backup.rs  process.rs
  ├─ fs.rs  settings.rs  error.rs  types.rs  commands.rs
src-tauri/tests/      # 引擎集成测试（7 项，覆盖核心契约）
src/                  # React 前端（Dashboard/Sessions/Directories/Backups/Settings）
```

## 开发

```bash
npm install --registry=https://registry.npmmirror.com   # 国内镜像加速
npm run dev          # 前端 dev server (1420)
cargo tauri dev      # 启动完整桌面应用（需 Rust 工具链）
```

## 构建 / 打包

```bash
cargo tauri build    # 产出 .app / .dmg（macOS）或 .exe / .msi（Windows）
```

产物位置：`src-tauri/target/release/bundle/`

## 测试

```bash
cd src-tauri
cargo test --test engine                       # 7 项引擎集成测试
OPENCODE_CLEANER_SKIP_PROC_CHECK=1 cargo ...   # 测试旁路进程检测
cargo run --example smoke                      # 真实数据冒烟扫描
```

## 关键实现要点（见 docs/ARCHITECTURE.md ADR-004）

- **删除一致性**：`DELETE FROM session` 触发 FK 级联（message→part, todo, session_share, session_message, session_input, session_context_epoch），但 **`event_sequence` 无 FK**，须显式 `DELETE FROM event_sequence WHERE aggregate_id IN (...)`，否则留孤儿致 opencode 异常。
- **`PRAGMA foreign_keys=ON`**：SQLite 默认 OFF，删除前必须开启，否则级联失效。
- **快照按项目存**：`snapshot/<project.id>/<hash>/`，跨会话共享，故仅支持项目级 / 全清。
- **会话 diff 按会话存**：`storage/session_diff/ses_xxx.json`，可随会话精确删除。
