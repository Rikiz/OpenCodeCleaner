# opencode cleaner

[![CI](https://github.com/Rikiz/OpenCodeCleaner/actions/workflows/ci.yml/badge.svg)](https://github.com/Rikiz/OpenCodeCleaner/actions/workflows/ci.yml)
[![Release](https://github.com/Rikiz/OpenCodeCleaner/actions/workflows/release.yml/badge.svg)](https://github.com/Rikiz/OpenCodeCleaner/actions/workflows/release.yml)

为 opencode（CLI 与 Desktop）瘦身的跨平台（macOS / Windows / Linux）桌面应用。扫描磁盘占用，安全清理历史会话、日志、快照与缓存。

三平台由 `.github/workflows/ci.yml` 的矩阵保证：`ubuntu-latest` / `macos-latest` / `windows-latest` 均跑 clippy 与引擎集成测试。

## 功能（v1）

- **占用扫描与可视化**：环形图展示各目录占比 + 会话占用排行，5 秒内完成。
- **历史会话清理**：按时间 / 大小 / 项目 / 关键词筛选，批量删除（含 DB 级联 + event 孤儿清理）；「保留最近 N 个」一键瘦身。
- **目录清理**：日志 / 快照 / session_diff / 缓存等，opencode 运行时也可清理。
- **安全机制**：opencode 运行检测（阻止 DB 操作）→ dry-run 预览精确可回收字节 → 清理前自动备份（DB checkpoint + 目录归档）→ VACUUM 回收空间；备份可一键恢复。
- **数据迁移**：Windows 上把会话/日志/快照/缓存整体搬到其它磁盘，原路径留 NTFS 目录联接，opencode 无需配置或重启。
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
src-tauri/tests/      # 引擎集成测试（14 项：核心契约 + 跨平台删除记账 + 数据迁移）
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
cargo tauri build    # 产出 .app / .dmg（macOS）、.exe / .msi（Windows）、.AppImage / .deb（Linux）
```

产物位置：`src-tauri/target/release/bundle/`

### 正式发版：打 tag 即出包

`release.yml` 监听 `v*` tag，在四个 target 上构建并挂到同一个 GitHub Release
（macOS arm64 dmg、macOS x64 dmg、Windows NSIS+MSI、Linux AppImage+deb）。
四个平台全部构建成功后，草稿才转为正式发布——不会出现「Release 里只有一半平台」的页面。

```bash
# 1. 同步版本号（必须三处都改）：src-tauri/tauri.conf.json、src-tauri/Cargo.toml、package.json
# 2. 本地校验（CI 里跑的是同一条）
npm run release:check && npm run release:check -- v1.0.1

# 3. 提交推送，然后打 tag
git tag v1.0.1 && git push origin v1.0.1
```

tag 与三处版本号不一致时 CI 会**直接失败**——否则会打出一个「自称 1.0.0 却挂在
v9.9.9 下」的包，下载页面上完全看不出问题。

需要补传某个平台产物时，在 Actions 面板对 `Release` 工作流点 *Run workflow*
手动重跑即可（此时以应用版本而非 tag 为准，注意别用改过版本号的分支跑）。

## 已知限制

- **产物未做代码签名**（没有 Apple Developer ID / Windows 代码签名证书）。
  macOS 首次打开需右键→「打开」或 `xattr -dr com.apple.quarantine`；
  Windows 需在 SmartScreen 里点「更多信息 → 仍要运行」。
- **没有自动更新**。设置页的「自动检查更新」开关目前未接实现（预留项），
  升级方式是手动下载新版本覆盖安装。

### 构建前置依赖

| 平台     | 必需依赖                                                                                                   |
| -------- | ---------------------------------------------------------------------------------------------------------- |
| macOS    | Xcode Command Line Tools                                                                                     |
| Windows  | Rust MSVC 工具链 + **Visual Studio Build Tools**（`rusqlite` 需 C 编译器）+ WebView2 Runtime（Win11 自带，Win10 1809+ 需引导安装） |
| Linux    | `build-essential` `pkg-config` `curl` `wget` `file` `libwebkit2gtk-4.1-dev` `libgtk-3-dev` `libayatana-appindicator3-dev` `librsvg2-dev` `libxdo-dev` `libssl-dev` `patchelf` |

Debian/Ubuntu 一键安装：

```bash
sudo apt-get install -y build-essential curl wget file pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev libssl-dev patchelf
```

> Linux 打包 AppImage 需要联网下载 linuxdeploy；离线环境请用 `--no-bundle` 只出二进制。

## 数据迁移（Windows，把数据挪到 D 盘）

设置页「数据迁移」卡片：opencode 在 Windows 上默认把会话、日志、快照、缓存写在系统盘
`%USERPROFILE%\.local\share\opencode`。点「迁移到其它磁盘」即可整体搬到 D 盘等其它卷。

原理是 **NTFS 目录联接**：文件搬到新位置后，原路径变成一个指向新位置的联接。
因此 opencode **不需要任何配置、不需要重启、不需要改环境变量**，CLI、桌面端和本清理器
都照旧使用原路径，透明落到新磁盘（见 `docs/ARCHITECTURE.md` ADR-010）。

流程与安全保障：

- **预检**：opencode 必须完全退出；目标磁盘可用空间 ≥ 数据量 ×1.5；实测两个卷是否都支持联接
  （exFAT/FAT32 会明确拒绝）。
- **复制 + 校验**：逐文件复制带进度，完成后校验文件数、字节数与数据库会话数三项一致，
  任何一项不符即中止且不切换。
- **切换**：原目录先改名让位，原路径建联接；建联接失败会自动把原目录改回去。
- **旧副本不自动删**：迁移完成后 C 盘仍留着完整旧副本，占用多少会明确显示，
  需你单独点「删除旧副本」才真正释放。删除不可撤销。
- **一键撤销**：把联接摘掉、旧副本换回原位。

注意事项：

- 目标磁盘必须是 **NTFS**（目录联接不支持 exFAT/FAT32）。
- 迁移期间请保持 opencode 完全退出；复制大目录可能需要几分钟。
- `~/.local/state/opencode`（运行时状态，通常很小）不迁移，UI 中有标注。
- 若缓存目录不存在（全新安装），会跳过它并在结果页提示，不影响数据目录迁移。
- 数据目录内若存在链接/特殊文件，复制阶段会直接报错并说明是哪个路径——不会静默跳过。
- 若误删了 D 盘上的目标目录，opencode 会报数据目录找不到——把目录恢复回去即可，
  或用「撤销迁移」回到原位置。

## 跨平台实现要点

- **路径解析三平台一致**：opencode 自身在三平台上都用 `XDG_DATA_HOME || ~/.local/share` 与 `XDG_CACHE_HOME || ~/.cache`，**Windows 下同样如此**（不是 `%APPDATA%`/`%LOCALAPPDATA%`）。本项目 `paths.rs` 与之逐字对齐，因此 Windows 上定位到 `C:\Users\<user>\.local\share\opencode`。
- **删除如实记账**：Windows 文件锁是强制的——一个被占用文件会让整棵递归删除失败，而 Unix 上其余文件照删。`fs::remove_dir_all_counted` 因此逐文件删除并累计**实际**回收字节，失败项进 `CleanResult.errors` 并在向导「完成」页展示，`freed_bytes` 不再是扫描估值。
- **DB 恢复不写坏文件**：先落到 `opencode.db.restore-tmp`，再删除原文件并 rename 换入；原文件被占用时原样保留并报明确错误。
- **按卷查空闲空间**：`GetDiskFreeSpaceExW`（Windows）/`df -k <path>`（Unix），数据盘与系统盘不一致时也不会误判。
- **进程检测**：进程名大小写不敏感并剥离 `.exe` 后缀，另按 exe basename 兜底。

## 测试

```bash
cd src-tauri
cargo test --test engine                       # 14 项引擎集成测试
cargo clippy --all-targets -- -D warnings      # 与 CI 一致的 lint 门禁
OPENCODE_CLEANER_SKIP_PROC_CHECK=1 cargo ...   # 测试旁路进程检测
cargo run --example smoke                      # 真实数据冒烟扫描
```

## 关键实现要点（见 docs/ARCHITECTURE.md ADR-004）

- **删除一致性**：`DELETE FROM session` 触发 FK 级联（message→part, todo, session_share, session_message, session_input, session_context_epoch），但 **`event_sequence` 无 FK**，须显式 `DELETE FROM event_sequence WHERE aggregate_id IN (...)`，否则留孤儿致 opencode 异常。
- **`PRAGMA foreign_keys=ON`**：SQLite 默认 OFF，删除前必须开启，否则级联失效。
- **快照按项目存**：`snapshot/<project.id>/<hash>/`，跨会话共享，故仅支持项目级 / 全清。
- **会话 diff 按会话存**：`storage/session_diff/ses_xxx.json`，可随会话精确删除。
