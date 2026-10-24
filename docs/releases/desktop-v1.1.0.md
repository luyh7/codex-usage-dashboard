# Desktop 1.1.0

Windows 与 Mac 免费直装客户端稳定版，包含匹配的统计程序，不需要安装 Node、Rust、Skill 或平台账号。

## 下载与支持范围

| 已验收环境 | 安装包 | SHA256 |
| --- | --- | --- |
| Windows 11 x64，build 22631，已有 WebView2 155.0.4283.62 | [Windows 安装器](https://github.com/luyh7/codex-usage-dashboard/releases/download/desktop-updates/cousash-1.1.0-windows-x64.exe) | `21ecfba83b0ee8b45255ebe3e6e9036d0d884f81760be7c58498a30184307b4c` |
| macOS 26.6.2（25G83），Apple Silicon arm64 | [Mac DMG](https://github.com/luyh7/codex-usage-dashboard/releases/download/desktop-updates/cousash-1.1.0-darwin-arm64.dmg) | `bdd4f509313b10b78070ce146db9d9508bfe8d64f41ee924a1529ec6fa7ff93e` |

稳定支持承诺仅限以上环境。完整校验清单见同一 Release 的 `SHA256SUMS`。

## 安装与更新

Windows 安装器没有付费应用签名；Mac 使用有效 ad-hoc 签名，没有 Developer ID 或 Apple 公证。首次打开可能显示 SmartScreen/Gatekeeper 提示，设备管理策略可能禁止运行。请核对下载来源与 SHA256，并按系统针对该应用的提示处理。

Mac 将应用复制到 Applications 后打开。此前 1.0.1 带 Chrome 隔离标记的包实际触发 Gatekeeper 拦截；用户仅对此已核对候选授权“仍要打开”并完成本机管理员认证后，首次启动、正常退出和再次打开均通过，Gatekeeper 保持启用。本轮 1.1.0 已检查新 DMG 的完整 ad-hoc 签名和原生启动，未重复浏览器隔离首开路径。按单个可信应用处理提示，不要求关闭全局安全保护。

客户端通过独立的 [GitHub Releases HTTPS 频道](https://github.com/luyh7/codex-usage-dashboard/releases/download/desktop-updates/latest.json)检查更新，安装前验证长期公钥对应的更新签名及发行版本。更新替换应用与随包程序，保留业务数据；另提供完整安装包用于手动升级和回退。Windows 自动更新显示一次安装语言确认，确认后继续安装与重启。

本次重新构建内部版本为 1.1.0 的生产包，重新生成绑定 1.1.0 的长期公钥更新签名。两端已通过全新安装、同版重装、卸载保留数据、已发布 1.0.1 完整包回退和 1.0.1 → 1.1.0 完整包升级；准确的 24,000 tokens、语言/时区、来源、设备身份、导入快照、目录记录及窗口尺寸保留。Windows 覆盖升级时旧宿主仍在运行，安装器正常停止本轮宿主和 helper。公开频道切换后另行记录真实 HTTPS 自动更新结果。

## 已知限制

- 当前 Windows 浏览器下载未带 `Zone.Identifier`，没有观察到 SmartScreen；标准 Internet Zone 拦截/放行路径未验收，不承诺其他安全策略下无需确认。
- Intel Mac、macOS 14.5 等旧系统、Windows 10、全新或离线 WebView2、独立 Linux 桌面不在本次稳定支持范围；Linux/WSLg 证据仅属实验验证。
- 多显示器、高 DPI 切换、睡眠唤醒、真实登录启动、长期运行，以及完整性能和提示框矩阵尚未完成验收。
- 历史 Windows 启动超时、一次未完整观察的 Windows GUI 宿主退出及 WSLg 堆损坏退出尚未定位根因；本轮监视重跑通过不代表这些风险已经修复。Windows 原生文件对话框须使用系统路径和反斜杠。

完整证据、制品来源和回退说明见[审查与验收记录](https://github.com/luyh7/codex-usage-dashboard/blob/main/docs/desktop-review-2026-10-24.md)及[桌面使用说明](https://github.com/luyh7/codex-usage-dashboard/blob/main/docs/desktop.md)。桌面版使用独立的 `desktop-updates` 频道；仓库的 CLI Latest 入口保持用于 CLI 发行。

源码标签：[desktop-v1.1.0](https://github.com/luyh7/codex-usage-dashboard/tree/desktop-v1.1.0)。

回退安装包：[Windows 1.0.1](https://github.com/luyh7/codex-usage-dashboard/releases/download/desktop-updates/cousash-1.0.1-windows-x64.exe)、[Mac 1.0.1](https://github.com/luyh7/codex-usage-dashboard/releases/download/desktop-updates/cousash-1.0.1-darwin-arm64.dmg)。保留用户配置、设备身份及导入数据，退出应用后安装完整旧版本；不要混用宿主与 helper。
