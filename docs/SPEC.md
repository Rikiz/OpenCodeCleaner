# SPEC: opencode-cleaner

## Metadata

| Field        | Value                          |
| ------------ | ------------------------------ |
| Version      | 1.0.0                          |
| Status       | Draft                          |
| Author       | —                              |
| Last Updated | 2026-08-01                     |
| Target       | opencode ≥ 1.17 (DB schema v2) |

---

## 1. Problem Statement

opencode（CLI 与 Desktop）长期使用后磁盘占用持续增长且不可逆。占大头的是 `opencode.db` 中累积的会话数据——尤其是工具输出全文（webfetch 抓取的网页 HTML、bash 大输出、read/write 的文件全文、推理 trace），以及日志、Git 快照仓库、模型包缓存。用户既无法直观了解占用分布，也没有安全的批量清理手段。手动删文件风险高（破坏 DB 一致性、留下孤儿数据、误删运行中实例的数据）。

## 2. Target Users

opencode 重度用户：日均多次交互、多项目并行，使用 macOS 或 Windows，磁盘敏感（笔记本用户）。

## 3. Goals (v1)

| ID  | Goal                       | Priority | Success Metric                                    |
| --- | -------------------------- | -------- | ------------------------------------------------- |
| G1  | 占用扫描与可视化分析       | P0       | 5 秒内完成全量扫描并渲染图表（10k 会话基线）      |
| G2  | 历史会话批量清理           | P0       | 支持按会话/时间/大小/项目筛选，批量删除含级联     |
| G3  | 日志/缓存/快照目录清理     | P1       | 分类一键清理非会话类数据                          |
| G4  | Dry-run 预览 + 自动备份    | P0       | 操作前展示精确可回收字节，自动备份可一键恢复      |
| G5  | opencode 运行中安全检测    | P0       | 检测到进程则阻止 DB 操作并引导退出                |

## 4. Non-Goals (v1)

- 不做 Part 级细粒度删减（如仅删 webfetch HTML、保留对话文本）——列为 v2
- 不做自动定期清理（计划任务）——列为 v2
- 不修改 opencode 自身代码或配置
- 不提供 Linux 安装包（路径逻辑与引擎完全兼容，AppImage/deb 打包列为 v1.1）
- 不做跨机同步 / 多盘数据分布（仅支持把数据整体搬到另一个本地卷）
- 不清理 `auth.json`/`account`/`credential` 等凭证（涉密且价值低）
- 不做会话导出/迁移（opencode 已有 `export`/`import` 命令）

## 5. Glossary

| Term           | Meaning                                                                 |
| -------------- | ----------------------------------------------------------------------- |
| data root      | opencode 主数据目录，默认 `~/.local/share/opencode/`                    |
| cache root     | opencode 缓存目录，默认 `~/.cache/opencode/`                            |
| session        | 一次 opencode 对话，DB `session` 表一行，ID 形如 `ses_xxx`              |
| part           | 会话内的内容片段，DB `part` 表，type 含 tool/text/reasoning/step 等     |
| snapshot       | opencode 的 Git 快照（undo 用），按 **项目** 存于 `data/snapshot/`      |
| session_diff   | 会话级 undo diff，按 **会话** 存于 `data/storage/session_diff/ses_xxx.json` |
| WAL            | SQLite Write-Ahead Log，opencode.db 使用 WAL 模式（伴随 -wal/-shm 文件）|
| dry-run        | 仅计算可回收空间、不执行任何写操作的预览模式                            |
| orphan         | 删除 session 后因无 FK 级联残留的数据（如 `event`/`event_sequence`）    |

## 6. Constraints

- **C1 锁约束**：opencode 运行时其 SQLite 处于 WAL 锁定，不可对其执行写操作（删除/VACUUM）。
- **C2 一致性约束**：删除必须保持 DB 引用完整性，不得留下孤儿行导致 opencode 启动异常。
- **C3 兼容性约束**：必须兼容 opencode 任意迁移状态下的 DB；只读取稳定表（session/message/part/project），不依赖易变字段。
- **C4 安全约束**：清理行为本身不得造成 opencode 启动崩溃；删除后 DB 仍可被 opencode 正常打开。
- **C5 体积约束**：安装包 < 30MB（Tauri 2 基准约 10MB）。
- **C6 权限约束**：仅需用户级文件系统读写权限，不提权。

## 7. Data Sources

| Source                       | Path (relative to root)        | Keyed by   | Override Env     | Safe while running |
| ---------------------------- | ------------------------------ | ---------- | ---------------- | ------------------ |
| **data root** `~/.local/share/opencode/` |                                |            | `XDG_DATA_HOME`  |                    |
| ↳ `opencode.db` + `-wal`/`-shm` | `opencode.db`               | —          |                  | 否（DB 写锁）       |
| ↳ 日志                       | `log/*.log`                    | 文件名时间戳 |                | 是                 |
| ↳ 快照仓库                   | `snapshot/<project.id>/<hash>/`| **项目**   |                | 是（独立 git 仓库）|
| ↳ 会话 diff                  | `storage/session_diff/ses_xxx.json` | **会话** |                | 是                 |
| ↳ tool-output                | `tool-output/`                 | —          |                | 是                 |
| ↳ repos                      | `repos/`                       | —          |                | 是                 |
| **cache root** `~/.cache/opencode/` |                           |            | `XDG_CACHE_HOME` |                    |
| ↳ 模型包缓存                 | `packages/`                    | —          |                | 是                 |
| ↳ 二进制                     | `bin/`                         | —          |                | 是                 |
| ↳ models.json                | `models.json`                  | —          |                | 是                 |

> 注：opencode 自带 snapshot prune（7 天 / 2MB 阈值），但 gitdir 目录本身不自动删除；清理器可清孤儿项目快照或全清。

## 8. User Stories

### US-1 首次打开，查看磁盘占用全貌
**As** opencode 用户
**I want** 打开应用后立即看到 opencode 各部分磁盘占用
**So that** 我了解清理的价值和优先级

**Acceptance Criteria**
- AC1.1 自动定位 data root 与 cache root（XDG + 环境变量覆盖）
- AC1.2 展示总占用(MB)、各子目录占比(环形图)、会话占用排行 Top 20(条形图)
- AC1.3 每个会话显示：标题、项目、最后活跃时间、part 数、占用 MB、模型
- AC1.4 展示「可回收空间预估」卡片（基于默认保留策略的 dry-run）
- AC1.5 扫描期间展示进度（当前目录/总目录）；扫描时间 ≤ 5s（10k 会话）
- AC1.6 若 opencode 正在运行，顶部显示警示条但不阻塞扫描

### US-2 按条件筛选并清理历史会话
**As** opencode 用户
**I want** 按时间/大小/项目筛选会话并批量删除
**So that** 我回收不需要的历史对话占用的空间

**Acceptance Criteria**
- AC2.1 筛选维度：时间范围（如 30 天前）、大小阈值、项目名、关键词
- AC2.2 支持「保留最近 N 个会话」一键选项
- AC2.3 勾选后 → dry-run 预览精确可回收字节 → 二次确认 → 执行
- AC2.4 删除走 FK 级联（message/part/todo/session_share/session_message/session_input/session_context_epoch）+ 显式删 event_sequence（无级联）
- AC2.5 同步删除对应 `storage/session_diff/ses_xxx.json`
- AC2.6 删除后执行 `VACUUM` 回收 DB 空闲页
- AC2.7 执行期间展示阶段进度（备份→删除→VACUUM）
- AC2.8 若 opencode 运行中，禁用执行按钮并提示退出

### US-3 清理非会话类数据
**As** opencode 用户
**I want** 一键清理日志/缓存/快照
**So that** 我回收非核心数据占用的空间

**Acceptance Criteria**
- AC3.1 分类展示：日志、session_diff、快照、缓存、tool-output、repos，各显示占用与安全等级
- AC3.2 每类可独立勾选；快照按项目粒度展示（项目名 + 占用）
- AC3.3 这些目录在 opencode 运行时**允许清理**（不触 DB 锁）
- AC3.4 清理后实时刷新占用数据

### US-4 清理前备份与预览
**As** opencode 用户
**I want** 清理前看到会回收多少空间并自动备份
**So that** 我心中有数，且可后悔

**Acceptance Criteria**
- AC4.1 Dry-run 输出选中项的**精确**可回收字节数（非估算）
- AC4.2 执行前自动备份：DB 先 `wal_checkpoint(TRUNCATE)` 落盘再复制；目录归档
- AC4.3 备份存于 `~/.local/share/opencode-cleaner/backups/<timestamp>/`，保留最近 5 份
- AC4.4 支持「从备份恢复」：替换当前 DB + 还原目录
- AC4.5 备份失败则中止清理并报错（fail-closed）

### US-5 检测 opencode 运行状态
**As** opencode 用户
**I want** 打开应用时自动检测 opencode 是否在运行
**So that** 我不会在清理 DB 时损坏数据

**Acceptance Criteria**
- AC5.1 检测进程名 `opencode` / `opencode-desktop`（跨平台，大小写不敏感）
- AC5.2 每 3 秒轮询，状态变化时 Tauri event 推送前端
- AC5.3 运行中：DB 相关操作（删会话/VACUUM）置灰 + 明确提示；目录类操作允许
- AC5.4 用户退出 opencode 后 3 秒内状态自动更新为可用

## 9. Non-Functional Requirements

| Category    | Requirement                                                        |
| ----------- | ------------------------------------------------------------------ |
| 性能        | 全量扫描 ≤ 5s（10k 会话）；删除 1k 会话 ≤ 10s；VACUUM 异步可取消   |
| 内存        | 峰值 < 200MB；会话列表虚拟滚动，不全量渲染                         |
| 启动        | 冷启动 ≤ 1.5s 到首屏                                               |
| 可靠性      | 任何清理操作失败不破坏现有数据（事务 + 备份前置）                   |
| 可观测      | 应用日志写入 `~/.local/share/opencode-cleaner/logs/`，可导出诊断   |
| 国际化      | v1 中英文双语；文案外置 i18n                                       |
| 无障碍      | 键盘可操作，图表有文字摘要，对比度 AA                              |

## 10. Privacy & Security

- **S1 本地优先**：所有数据读取与清理在本地完成，不发起任何网络请求（除应用自身更新检查，可关闭）。
- **S2 敏感数据不外传**：`part.data` 可能含 API key、源码、密钥；扫描结果仅聚合字节数与元数据（标题/项目/时间），**不回传内容**；UI 不展示 part 正文。
- **S3 凭证不触碰**：`auth.json`、`account`、`credential`、`account_state` 表列为只读禁区，清理器不读不删。
- **S4 备份加密**：备份含原始 DB，与 opencode 同等敏感；存于用户私有目录，权限 0600。
- **S5 无遥测**：v1 不采集任何使用数据。

## 11. OS Support Matrix

| OS              | Version           | Arch            | Backend        |
| --------------- | ----------------- | --------------- | -------------- |
| macOS           | 12 Monterey+      | arm64, x86_64   | system WebView |
| Windows         | 10 1809+ / 11     | x86_64          | WebView2 (Edge)|
| Linux           | glibc 2.31+（含 webkit2gtk-4.1 的发行版，如 Debian 12 / Ubuntu 22.04 / Fedora 39） | x86_64, aarch64 | system WebKitGTK |

三平台共用同一份引擎代码，差异仅在：`paths::free_space` 的实现（Win `GetDiskFreeSpaceExW` / Unix `df -k`）、
`db::detect_opencode_version` 的可执行名解析（Windows 需处理 `.exe`/`.cmd`/`.bat` shim）、
以及 `main.rs` 的 `windows_subsystem = "windows"`（避免 release 版弹出控制台）。
Linux 无官方安装包，但 `npx tauri build --no-bundle` 可直接产出可执行文件，CI 亦覆盖其编译。

Windows 另提供**数据迁移**能力：把 opencode 数据整体搬到其它本地卷，原路径留 NTFS 目录联接（ADR-010）。

## 12. Versioning & Compatibility

- 应用语义化版本（SemVer）。
- 对 opencode DB：**只读稳定表**（session/message/part/project），读取前探测列存在性，缺失列降级为空值而非崩溃。
- 对 opencode 大版本变更：若 DB schema 不兼容，扫描仍可用，清理按钮置灰并提示「待适配 opencode X.Y」。
- 维护 `opencode_versions_tested` 配置项记录已测版本。

## 13. Risks & Mitigations

| Risk                                    | Severity | Mitigation                                              |
| --------------------------------------- | -------- | ------------------------------------------------------- |
| 运行中清理 DB 导致损坏                  | 高       | 进程检测 + 操作前二次校验；fail-closed                  |
| 删除留 event 孤儿致 opencode 异常       | 中       | 显式删 event_sequence（契约 §4）                        |
| opencode 升级改 schema 致清理器崩溃     | 中       | 探测列存在性 + 降级；版本兼容性检查                      |
| 备份占满磁盘                            | 中       | 保留最近 5 份；备份前检查可用空间（需 < 备份大小 ×1.5）  |
| 误删正在进行的会话                      | 高       | 默认锁定最近 24h 会话不可选；二次确认                    |
| WebView2 在精简版 Windows 缺失          | 低       | 安装器内置 WebView2 引导                                |
| Windows 强文件锁导致整棵目录删不掉      | 高       | 逐文件删除 + 如实记账 `freed_bytes`；失败项进 `errors` 并在 UI 展示（ADR-008） |
| 数据在非系统盘导致空间预检误判          | 中       | 按路径所在卷查询空闲空间（`GetDiskFreeSpaceExW`）        |
| 覆盖 DB 失败致 opencode.db 损坏         | 高       | 暂存 `.restore-tmp` 后 rename 换入，失败保留原文件       |
| 跨卷迁移中途失败留下不一致副本         | 高       | 复制→校验（文件数/字节/会话数）→才切换；切换失败自动回滚；原目录永不自动删（ADR-010） |
| 目标卷不支持 NTFS 联接（如 exFAT）      | 中       | 预检阶段实测建联接并给出明确拒绝原因，不进入复制阶段   |

## 14. Dependencies

- `opencode` CLI ≥ 1.17（可选：`session list/delete` 作为兜底）
- 系统文件系统读写权限（用户级）
- Windows: WebView2 Runtime（Win11 自带，Win10 需引导安装）
- Windows: Visual Studio Build Tools（`rusqlite` bundled 需 C 编译器）
- Linux: webkit2gtk-4.1 等 Tauri 构建依赖（见 README 前置表）
