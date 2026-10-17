# ARCHITECTURE: opencode-cleaner

## 1. System Context

```
┌─────────────────────────────────────────────────────────────┐
│                      opencode-cleaner                        │
│                                                              │
│  ┌──────────────┐      ┌─────────────────────────────────┐  │
│  │  React Web   │◄────►│  Tauri 2 Bridge (commands)      │  │
│  │  (UI layer)  │ IPC  │  invoke("scan") / "execute" ... │  │
│  │  纯展示+状态  │      │  + event push (progress/status) │  │
│  └──────────────┘      └──────────────┬──────────────────┘  │
│                                       │                      │
│                ┌──────────────────────▼──────────────────┐   │
│                │           Rust Core Engine              │   │
│                │  (headless, fully unit-testable)        │   │
│                │                                         │   │
│                │  ┌──────┐ ┌───────┐ ┌────────────────┐  │   │
│                │  │ scan │ │ clean │ │    backup       │  │   │
│                │  └──┬───┘ └───┬───┘ └─────┬──────────┘  │   │
│                │     │         │           │             │   │
│                │  ┌──▼─────────▼───┐ ┌─────▼─────────┐   │   │
│                │  │     db.rs      │ │   process.rs   │   │   │
│                │  │  (rusqlite)    │ │  (sysinfo)     │   │   │
│                │  └──────┬─────────┘ └────────────────┘   │   │
│                │  ┌──────▼─────────┐                      │   │
│                │  │     fs.rs      │                      │   │
│                │  │  (dir ops)     │                      │   │
│                │  └────────────────┘                      │   │
│                └─────────────────────────────────────────┘   │
└──────────────────────────┬───────────────────────────────────┘
                           │ 只读 + 受控写
              ┌────────────▼──────────┐  ┌───────────────────┐
              │  opencode.db (SQLite) │  │ ~/.local/share/   │
              │   WAL 模式            │  │ opencode/*        │
              └───────────────────────┘  │ ~/.cache/opencode │
                                         └───────────────────┘
```

## 2. Architectural Decisions (ADR)

### ADR-001: 选用 Tauri 2
**Decision**：Tauri 2 (Rust) 作为桌面壳。
**Rationale**：安装包 ~10MB vs Electron ~100MB；Rust 侧 `rusqlite` 直连 SQLite 无需 Node 原生模块；`sysinfo` crate 跨平台进程检测；内存低适合工具类应用。
**Trade-off**：依赖系统 WebView（macOS WebKit / Windows WebView2），前端调试生态略弱于 Electron。

### ADR-002: 核心引擎与 UI 完全分离
**Decision**：扫描/清理/备份/进程检测全部在 Rust 侧实现，Tauri command 仅做薄封装，前端零业务逻辑。
**Rationale**：Rust 核心可独立单测（无需启动 WebView）；文件系统副作用集中可控；后续可复用为 CLI。
**Consequence**：所有副作用经 Rust，前端通过 invoke + event 驱动。

### ADR-003: DB 操作分级策略
**Decision**：只读与写操作分离。
- **只读**：`rusqlite` 以 `query_only=1` 打开，扫描/聚合用。
- **写（删除/VACUUM）**：要求 opencode 退出后执行；打开时 `PRAGMA foreign_keys=ON`（SQLite 默认 OFF，否则级联失效）；事务包裹。
- **兜底**：直连 SQL 失败时降级调用 `opencode session delete <id>`。
**Rationale**：直连可精确聚合占用量；启用 foreign_keys 是级联删除正确的硬性前提。

### ADR-004: 删除一致性与孤儿处理
**Decision**：删除 session 后显式清理无 FK 级联的关联表。
**Background**：实测 `event_sequence.aggregate_id = session_id` 但无 FK 级联；`session_diff` 为文件（`storage/session_diff/ses_xxx.json`）；`part` 经 `message_id` 二级级联。
**Cleanup set**（一次删 session 后）：
- DB 级联（自动）：`message`→`part`、`todo`、`session_share`、`session_message`、`session_input`、`session_context_epoch`
- DB 显式：`DELETE FROM event_sequence WHERE aggregate_id IN (...)`（→ 级联 `event`）
- 文件：删 `storage/session_diff/ses_xxx.json`
- 快照：**不按会话删**（快照按项目存，跨会话共享），仅支持项目级或全清
**Rationale**：保证删除后无孤儿、opencode 重启不异常。

### ADR-005: 备份策略
**Decision**：清理前将受影响数据复制到应用备份目录 `~/.local/share/opencode-cleaner/backups/<timestamp>/`。
- DB：`PRAGMA wal_checkpoint(TRUNCATE)` 落盘后整库复制（含 -wal/-shm 已合并）
- 目录：tar 归档（.tar.gz）
**Retention**：保留最近 5 份，超限删最旧；备份前校验可用磁盘空间 ≥ 备份预计大小 ×1.5。
**Fail-closed**：备份失败则中止清理。

### ADR-006: 进程检测策略
**Decision**：`sysinfo` 启动时 + 操作前双重检测；每 3s 轮询，状态变化经 Tauri event 推送。
- 检测进程名 `opencode` / `opencode-desktop`（大小写不敏感）
- 运行中：DB 操作置灰；目录类操作（日志/缓存/快照）允许
**Trade-off**：3s 轮询有微小窗口期，操作前再校验一次兜底。

### ADR-007: 前端状态管理
**Decision**：React + Zustand（轻量）+ TanStack Query（服务端状态缓存/重试）。
- 扫描结果作为 Query 缓存，invalidate 触发重扫
- 选中态/筛选态用 Zustand local store
- Tauri event 订阅进度与进程状态

## 3. Component Overview

| Component       | Location          | Responsibility                                       |
| --------------- | ----------------- | ---------------------------------------------------- |
| `paths`         | `src-tauri/src/`  | 定位 opencode data/cache root（XDG + 覆盖 + 探测）   |
| `scan`          | 同上              | fs 遍历 + DB 聚合 → `ScanReport`                      |
| `db`            | 同上              | SQLite 只读查询 / 写删除事务 / VACUUM / checkpoint   |
| `clean`         | 同上              | 接收 `CleanSelection` → dry_run / execute 编排        |
| `backup`        | 同上              | DB checkpoint+复制 / 目录归档 / 恢复 / retention     |
| `process`       | 同上              | 跨平台进程检测 + 轮询                                |
| `fs`            | 同上              | 目录大小/删除/归档原语                               |
| `settings`      | 同上              | 应用自身设置持久化（JSON）                           |
| `commands`      | 同上              | Tauri `#[command]` 桥接 + event 发射                 |
| `Dashboard`     | `src/screens/`    | 总览：环形图 + 会话排行 + 可回收卡片                  |
| `Sessions`      | 同上              | 会话列表/筛选/批量删                                 |
| `Directories`   | 同上              | 日志/缓存/快照 分类清理                              |
| `Backups`       | 同上              | 备份列表/恢复                                        |
| `SafetyBar`     | `src/components/` | 运行检测警示条                                       |
| `CleanWizard`   | 同上              | dry-run 预览 → 确认 → 进度 → 结果                    |

## 4. Data Flow

### 4.1 扫描流程
```
App start
  → commands::scan()
    → paths::resolve()                    // 定位 data/cache root
    → process::check()                    // 顺带检测进程
    → emit("scan-progress", {dir, i, n})
    → fs::scan_dirs(root)                 // log/snapshot/session_diff/cache...
    → db::open_readonly() → db::scan_sessions()
    → 组装 ScanReport
  → front: TanStack Query 缓存 → 渲染 Dashboard
```

### 4.2 清理流程
```
User 选会话 + 点清理
  → commands::dry_run(selection)
    → db::estimate(selection) + fs::estimate(selection)
    → 返回 CleanPreview（精确字节）
  → front: CleanWizard 展示预览

User 确认
  → commands::execute(selection)
    → process::check() → 运行中? Err(ProcessRunning)  // 操作前再校验
    → backup::create(affected)
        → db::checkpoint(TRUNCATE) → copy db
        → fs::archive(affected_dirs)
    → db::open_readwrite()
        → PRAGMA foreign_keys=ON; BEGIN
        → DELETE FROM session WHERE id IN (?)
        → DELETE FROM event_sequence WHERE aggregate_id IN (?)
        → COMMIT
    → fs::remove(session_diff files)
    → db::vacuum()
    → emit("clean-progress", phase, i, n) 各阶段
    → 返回 CleanResult
  → front: invalidate scan → 刷新 Dashboard
```

### 4.3 进程状态流
```
process::poll_loop (每 3s)
  → 状态变化? emit("process-status", {running, names})
  → front: SafetyBar 更新，DB 操作按钮 enable/disable
```

## 5. Concurrency & Async

- Tauri command 标 `async`，重操作在 `tokio::task::spawn_blocking`（rusqlite 是同步阻塞）。
- 长任务（扫描/VACUUM）通过 `tauri::AppHandle::emit` 推进度，前端可取消（任务持有 `CancellationToken`）。
- 同一时刻只允许一个写操作（`clean::Mutex`），并发 invoke 直接拒绝。

## 6. Observability

- 应用日志：`~/.local/share/opencode-cleaner/logs/app-YYYYMMDD.log`，滚动 7 天。
- 日志级别：INFO 默认，DEBUG 可在设置开启；`--verbose` 启动参数。
- 每次清理产出执行报告 JSON（已删会话、回收字节、耗时、错误），存于备份目录便于诊断。
- 设置页「导出诊断包」：打包最近日志 + 系统信息 + DB schema 摘要（不含数据）。

## 7. Security & Privacy

- 无网络出站（除可选应用更新检查）。
- 扫描仅聚合元数据，不读 `part.data` 正文，UI 不展示会话内容。
- 凭证禁区：`auth.json` / `account` / `credential` / `account_state` / `control_account` 不读不删。
- 备份目录权限 0600；恢复操作覆盖前二次确认。

## 8. Testing Strategy

| Layer      | Tool          | Scope                                        |
| ---------- | ------------- | -------------------------------------------- |
| Rust 单元  | `cargo test`  | scan/db/clean/backup/process，用构造副本 DB  |
| Rust 集成  | `cargo test`  | 端到端 dry_run→execute→restore，临时目录模拟 |
| 前端单元   | Vitest        | 纯函数、状态 reducer、格式化                  |
| 组件       | Vitest+RTL    | 关键交互（筛选/勾选/向导步骤）               |
| E2E        | Playwright    | mock Tauri 后端跑通核心流程                  |
| 跨平台 CI  | GitHub Actions| macos-latest + windows-latest 矩阵           |

测试夹具：`test/fixtures/seed.sql` 在临时 DB 重建固定数据；`test/fixtures/fs/` 模拟目录结构。

## 9. Project Structure

```
opencode-cleaner/
├─ docs/                  # SPEC / ARCHITECTURE / CONTRACTS（本目录）
├─ src-tauri/             # Rust 核心引擎 + Tauri 命令层
│  ├─ src/
│  │  ├─ paths.rs
│  │  ├─ scan.rs
│  │  ├─ db.rs
│  │  ├─ clean.rs
│  │  ├─ backup.rs
│  │  ├─ process.rs
│  │  ├─ fs.rs
│  │  ├─ settings.rs
│  │  ├─ error.rs         # CleanerError 枚举
│  │  └─ commands.rs      # #[tauri::command]
│  ├─ tests/              # 集成测试
│  └─ test/fixtures/      # seed.sql + fs 模拟
├─ src/                   # React 前端
│  ├─ screens/
│  ├─ components/
│  ├─ lib/                # invoke 封装、types（与 Rust 对齐）
│  ├─ store/              # Zustand
│  └─ i18n/
├─ e2e/                   # Playwright
└─ .github/workflows/     # CI 矩阵
```
