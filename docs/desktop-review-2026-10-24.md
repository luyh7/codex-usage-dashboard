# PR #9 审查与 Mac 实机补验

审查基线为 main 的 38b9d03 至 PR 原始 HEAD 7f1fde5（15 个提交、75 个文件）。规范与需求两条审查分别执行。规范审查没有确认的违规；需求审查确认并修复三项 P2 来源一致性问题，应用源码提交为 **d54921d**。修复后的独立复查确认三项均解决，没有发现新引入的可操作缺陷。

## 审查修复

| 问题 | 修复后的行为 |
| --- | --- |
| 自动发现来源仅用空的参数列表比较，改变 CODEX_HOME 后可能复用旧索引 | 服务记录实际解析后的来源；启动时用同一解析器生成有界的来源摘要，比较真实来源与构建身份 |
| summary/export 直接复用健康实例，绕过来源和版本检查 | 两个命令统一经过共享兼容检查；冲突时保留运行实例，并在创建或替换导出目标前拒绝 |
| 来源配置先保存，再检查独立服务归属 | 原生命令先检查归属，再通过服务保存来源并协调重启；停止失败尝试恢复原来源，拒绝时不显示成功提示 |

回归用例 `automatic and saved source changes reject stale services without exporting their data` 在原版辅助程序上失败（错误地成功退出 0），在修复版通过。另覆盖模拟 WSL 环境中相同实际来源跨发现模式的复用，防止仅按发现开关误拒绝。

来源检查只在隐藏的构建身份握手中返回固定长度摘要，普通 `--build-info` 输出不变。32 个绝对来源路径合计约 28 KB 的检查输出为 192 bytes，摘要查询成功；这不是对超过系统路径长度上限的支持承诺。

## Mac 实机

环境为 macOS 26.6.2（25G83）、Apple Silicon arm64、Xcode 26.6。使用当前 worktree 构建的生产配置 app/dmg，设置 `APPLE_SIGNING_IDENTITY=-`，没有启用 desktop-tests。应用复制到用户 Applications 下带空格的目录，从无关工作目录运行；app 及辅助程序完整 ad-hoc 签名检查通过。没有 Developer ID 或公证。

随包服务 build ID：`3e9a54dc1065e9a410cd3b620eb59df29896a8bdaea1bc5b5ca441c6c4e388b7`。验收仅使用隔离的虚构日志和快照。

已实际通过：

- WKWebView 看板、中文项目、会话详情、准确的 12K 用量及 08:00 时间展示；设置以外没有额外时区标签。
- Cmd+, 打开设置；原生文件保存框的首次取消不写目录记录；中文/空格目录完整及脱敏导出均成功，脱敏对话框复用成功目录。解压检查确认完整快照保留虚构消息，脱敏快照移除消息。
- 原生来源选择框首次定位当前来源；选择新的中文目录后，客户端自己重启服务，端口改变，界面用量从 12K 变为 24K，语言和展示时区保留。
- 连接独立 CLI 服务时，恢复自动发现显示明确错误，preferences.json 逐字节不变，原服务继续提供 24K 数据。
- 通过系统文件选择框导入 12K 脱敏快照，设备列表出现导入设备，汇总成为 2 会话、36K。
- Cmd+Q 退出自己拥有的宿主和服务后，两个运行记录均清除。关闭窗口后 CLI open 可恢复原宿主窗口；没有把此检查记为 Dock 点击验证。
- 另从 Finder 的 Applications 入口再次打开已运行的应用，原窗口及 36K 数据恢复；实际应用菜单显示窗口、设置、重新连接和退出入口，选择“退出”后宿主、服务及记录均清理。此轮没有验证 Finder 冷安装/移除或 Dock 点击。
- 服务正常退出后，宿主自动恢复，宿主 PID 保持、服务 PID 改变，导入数据和 36K 保留；单次恢复约 3.7 秒。
- 单实例、同配置并发冷启动、双配置隔离、退出隔离和窗口尺寸往返通过；原生协议、设置、来源冲突、HTTP 安全和离线安装器检查通过（7 项，Windows 专属项跳过）。
- 图表键盘检查可打开用量提示框，内容没有时区标签，Escape 关闭提示框并保留详情；已保存打开状态截图。
- 800×650 窗口下详情弹层和键盘图表提示通过，提示框保持在视口内，Escape 先关闭提示再保留详情。通过界面切换 English 和 UTC 后关闭重开，端口改变，设置和 24M 保留；图表时间相应由 08:00 改为 00:00，没有在设置外增加时区标签。
- 中文/空格的 Applications 安装路径下，1,000 会话、100,000 条事件、12M 用量的完整及脱敏原生导出通过，解压分别为 48,682,699 / 46,206,183 bytes；逐项核对会话数、事件数、用量和消息脱敏。通过原生选择框重新导入脱敏快照后为 2,000 会话、24M。
- 同规模合成负载的单次冷启动为 2,345 ms，5 次摘要请求的中位数为 0.6 ms。这是本机单轮观测，不是跨平台性能承诺。
- 当前宿主源码在隔离的临时签名更新产品中通过全部六项更新/恢复检查：离线、下载中断、错误签名、版本不符、已签名但无效的安装包，以及成功更新/重启。升级至 1.0.2 后设置、设备身份、导入快照、目录记录和 24K 保留，宿主退出码 0；测试产品和密钥不进入生产包。测试后已重建正常 1.0.1 生产配置包。

本地证据保存在忽略的 output/verification/：mac-native-settings.png、mac-source-rejection.png、mac-keyboard-chart-tooltip.png、mac-narrow-keyboard-tooltip.png、mac-language-timezone-reopen.png、mac-source-switch.json、mac-independent-source-rejection.json、mac-service-recovery.json、long-source-inspection.json、desktop-concurrency-darwin.json、desktop-performance-darwin-arm64.json、mac-large-native-export.json、mac-large-import.json、mac-large-reopen.json、mac-settings-persistence.json、desktop-update-e2e-darwin.json。临时日志、截图、运行令牌、用户数据和签名密钥不加入 Git。

本机正常 1.0.1 arm64 DMG 的 SHA256 为 `ceeb097908862d99793da9581f32c91a1cfd115a39903a0d767f90f9121a7526`，位于 `desktop/target/aarch64-apple-darwin/release/bundle/dmg/Codex Usage Dashboard_1.0.1_aarch64.dmg`。本地包与 CI 包分别构建，摘要及 build ID 不混用。

## 当前验证与边界

本机 Rust workspace 57 项测试、桌面目录记录 3 项测试、前端 49 项测试、Svelte 检查、workspace/桌面 all-feature clippy、格式检查及发布/更新清单 6 项测试已通过。新增来源回归在当前 arm64 辅助程序通过。

当前源码的云端矩阵：

- [Desktop clients 40064637199](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199)：四目标全部通过，包含生产打包、格式/all-feature clippy、目录记录及 workspace 测试、协议、安全、启动/并发和包校验。Mac 两架构均通过全部六项签名更新/恢复检查，升级后保留 24K 及目录记录；Intel 报告的宿主源码摘要与本地 `dcf923bd…` 一致，宿主退出码 0，经历 3 次导航前置拒绝后正常继续。
- [Native compatibility 40064637196](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637196)：四目标和候选组装全部通过，发布及公开下载步骤跳过。

收尾时保留了远端并行加入的 CI 共享构建、缓存及推送差异路由（7100acd、8b2d13d）；应用源码相对 d54921d 没有变化。本地 15 项路由、发行和更新清单测试通过；[统一 Client CI 40065344097](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40065344097) 的共享检查、Native 四目标、Desktop 四目标及候选组装也全部通过，正式发布步骤跳过。新的工作流检查分工见 [CI 说明](development/ci.md)。

当前源码 d54921d 的桌面开发制品已生成，ZIP 摘要来自对应 Actions artifact 元数据；包内文件摘要另见各自的 artifacts.json：

| 目标 | 下载 | ZIP SHA256 |
| --- | --- | --- |
| Windows x64 | [NSIS](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199/artifacts/12549167391) | `dfed6f81d110d2e7745fa7ad7308585dceaf077fb07940220b5697a644254811` |
| macOS arm64 | [app / dmg](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199/artifacts/12549390279) | `76ee7626137c6a9e93354b19f8fbe4137366626f413eabedcd3c51188b56cbb1` |
| macOS x64 | [app / dmg](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199/artifacts/12549043948) | `a425e8ceb9618196a63776c1d9c9164147c3afb19d88fc14cab6d52d93f95688` |
| Linux x64 | [AppImage / deb](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199/artifacts/12549361635) | `9df783681d192bff1972e930a5223fb587d3ca153f8dfc837168e0d7cb563168` |

此前 Windows/Linux 的 GUI、安装和大快照证据仍见 [原验证记录](desktop-verification.md)，属于对应历史源码。新改动不能只凭旧包结果宣称跨平台验证完成。

仍需独立环境或外部资源的项目：Intel Mac GUI、最低 macOS 14.5、Finder 安装/移除与 Dock 菜单完整矩阵；Windows 10/全新或离线 WebView2；独立 Linux 桌面及无托盘交互；真实登录、多显示器、高 DPI 切换、睡眠唤醒及长期运行；极限快照规模和完整提示框性能矩阵。正式 Windows 签名、macOS Developer ID/公证及正式更新渠道仍缺资源。保留此前未定位的 Windows 启动超时和 WSLg 堆损坏风险，不以本轮通过宣称根因已经修复。

PR 保持草稿；没有合并、打发布标签或正式发布。
