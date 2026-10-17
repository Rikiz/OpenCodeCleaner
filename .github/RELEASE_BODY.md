## 下载

| 平台 | 文件 | 说明 |
| --- | --- | --- |
| macOS (Apple Silicon) | `*-aarch64.dmg` | 拖入「应用程序」即可 |
| macOS (Intel) | `*-x64.dmg` | 同上 |
| Windows | `*-setup.exe` / `*.msi` | 两者任选，功能相同 |
| Linux | `*-amd64.AppImage` | `chmod +x` 后直接运行 |
| Linux | `*.deb` | Debian / Ubuntu：`sudo apt install ./<文件名>.deb` |

## ⚠️ 本版本未做代码签名

这是个人维护的开源工具，没有购买 Apple Developer ID 与 Windows 代码签名证书，
因此系统会拦截未签名的程序。这是**预期行为**，不是文件损坏。

**macOS**：首次打开会提示「无法验证开发者」。

```bash
# 方式一：命令行放行一次
xattr -dr com.apple.quarantine "/Applications/opencode cleaner.app"
```

方式二：在 Finder 里**右键点击 → 打开** → 弹窗里再点「打开」。

**Windows**：SmartScreen 会弹「Windows 已保护你的电脑」，
点「更多信息」→「仍要运行」。

如果在意这一点，可以自行编译：仓库里有完整的 Rust + Node 构建说明
（需要 Rust 工具链；Windows 另需 Visual Studio Build Tools，Linux 另需 webkit2gtk-4.1 等）。

## 升级说明

本工具**不会自动更新**，也不会联网。升级方式就是下载新版本覆盖安装；
清理器自身的设置、备份与日志保存在 `%LOCALAPPDATA%\opencode-cleaner`
（macOS/Linux 为 `~/.local/share/opencode-cleaner`），升级不会影响它们。

⚠️ 如果你用过「数据迁移」把 opencode 数据搬到了其它磁盘，
**不要卸载或清理掉目标目录**——原路径只是指向它的目录联接。
详见应用内「设置 → 数据迁移」。