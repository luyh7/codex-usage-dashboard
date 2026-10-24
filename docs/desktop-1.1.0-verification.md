# Desktop 1.1.0 版本修正与升级验收

2026-10-24，用户明确要求将 PR #9 的桌面功能发行修正为 1.1.0，重新构建、签名并验收从已发布 1.0.1 升级，完成后发布稳定版，保留已知限制和回退包。修正由 [PR #10](https://github.com/luyh7/codex-usage-dashboard/pull/10) 承载，最终公开频道检查结果记在该 PR 的 Evidence 中。

## 制品与支持范围

生产产品标识仍为 `io.github.luyh7.cousash`，未启用 `desktop-tests`。支持承诺仍仅为 Windows 11 x64/build 22631/已有 WebView2 155.0.4283.62，以及 macOS 26.6.2（25G83）arm64。包内部宿主、helper、Tauri、Cargo 和 npm/skill 元数据统一为 1.1.0，没有业务数据迁移。

Windows 为未购买系统签名的 NSIS；Mac app 与 helper 的完整 ad-hoc 签名通过。更新签名使用原长期密钥，公钥与固定 HTTPS 入口不变，签名绑定 `version:1.1.0`。两端文件都通过公钥密码学验证，篡改字节被拒绝。私钥和口令只留在维护者 Mac 的受保护目录，没有传到 Windows、仓库或制品。

| 1.1.0 文件 | SHA256 |
| --- | --- |
| `cousash-1.1.0-windows-x64-setup.exe` | `21ecfba83b0ee8b45255ebe3e6e9036d0d884f81760be7c58498a30184307b4c` |
| `cousash-1.1.0-macos-arm64.dmg` | `bdd4f509313b10b78070ce146db9d9508bfe8d64f41ee924a1529ec6fa7ff93e` |
| `cousash-1.1.0-macos-arm64.app.tar.gz` | `927c314102d0f6aeb3f46a47cbba0a3704e898425867d3a583d29b177738175e` |

Mac helper build ID 为 `4cc59a3099dfe7c66165fc1a09249967e63de56dc2d85453c6f3aeb2e433fc6d`；Windows 为 `92ef5bc8741c4750c7883af8028e9f3f4df7e81d19967076360ce0b6bc4ea27b`。

## 已发布 1.0.1 的真实产品升级

基线使用此前已公开的完整生产包，不重新编译一个同号替代品：Windows SHA256 为 `88f83391937882397fb41dc92496a03fec66eeda152ddf115721646a7cb5ea74`，Mac DMG 为 `2ad0b9b6e2c3baab905ce74a32ed3cae775f1877188dd673c6c8a93eb2bd0e93`。

两端都已通过 1.1.0 全新安装、同版重装、卸载保留数据、1.0.1 完整包回退和 1.0.1 → 1.1.0 完整包升级。原生设置界面均显示 1.1.0。准确的 24,000 tokens、语言/时区、来源、设备身份、导入快照、目录记录和窗口尺寸保留。Windows 升级时基线宿主仍在运行，安装器停止本轮宿主及 helper，监视的宿主均正常退出。

发布按一个版本一个 Release 整理：桌面安装包、命令行程序、更新清单和校验文件都在 `vVERSION` 下。下载名使用 `macos`，不附构建码；完整构建身份及 SHA256 保留在 `manifest.json` 和 `SHA256SUMS` 中。 用户选择移除独立更新频道，不保留兼容入口。已发布 1.0.1/1.1.0 客户端内置的旧地址将停止工作，需要手动下载完整安装包升级；保留配置和导入数据。后续构建使用统一入口 `https://github.com/luyh7/codex-usage-dashboard/releases/latest/download/latest.json`，并继续验证长期公钥更新签名及发行版本。此轮整理复用已验收的程序字节，未重新构建或扩大稳定支持范围。 原始签名与旧清单保留在本次迁移的本地审计备份中。

## CI 与限制

版本一致性检查与 20 项发行/路由/清单测试通过。首次 CI 的 Mac 隔离更新构建暴露了测试脚本只替换 `1.0.x` 的问题，实际 helper 为 1.1.0，却断言测试基线为 1.0.1。修正只作用于隔离测试源的版本替换，保持其 1.0.1 → 1.0.2 测试矩阵；不改变生产应用或已验收字节。修正后的完整 [Client CI 40091756932](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40091756932) 与最终发行状态见 PR #10。

全部既有未验收项和历史未定位风险继续保留，详见 [1.1.0 发行说明](releases/desktop-v1.1.0.md)与 [1.0.1 原验收记录](desktop-review-2026-10-24.md)。1.1.0 没有重复 Windows 标准 Internet Zone/SmartScreen 或 Mac 浏览器隔离首开路径；不将 1.0.1 的单应用 Gatekeeper 例外视为对新包的自动放行保证。

本地证据位于忽略目录 `output/release-1.1.0/`：生产包、签名验证、两端报告、原生版本与更新界面、公开下载校验。合成数据、运行令牌与签名私钥不提交。
