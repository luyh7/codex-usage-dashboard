# PR #9 审查与发行准备验证

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

## 当前 Windows 与 Linux 补验

本轮从统一 Client CI 40065344097 下载 Windows、Linux 开发制品，核对 ZIP 摘要后在独立临时目录运行；应用源码相对 d54921d 未变，没有用历史包替代当前来源修复验证。Windows 为 Windows 11 x64（build 22631），现有 WebView2 155.0.4283.62；Linux 为 Ubuntu 26.04 / WSL2 / WSLg、WebKitGTK 2.52.6、GTK 3.24.52，不能当作独立 Linux 桌面验收。既有远端工作树及业务数据不改动，所有日志与快照均为隔离合成数据。

| 当前下载 | ZIP SHA256 |
| --- | --- |
| [Windows Desktop 12549362831](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40065344097/artifacts/12549362831) | `57aa2548714a2f8efb0e0819a8a4030577c4ea68cbcddbc5928e46762e2982a9` |
| [Linux Desktop 12549328028](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40065344097/artifacts/12549328028) | `54ee4723e6227af58dac3f95b668c2682e0972cf51e6fef52f5f7dd2d0ebc79e` |
| [Linux Native 12548833968](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40065344097/artifacts/12548833968) | `20672c1addd8e1c467c5606a420b04ac5f7c3affba5375784496efd979a86671` |

Windows 随包 helper 的 build ID 为 `bca396102ddd0d8d25cdad285fa37bd6154f832da25b636f0e8933dff95a4691`；Linux 为 `3e9a54dc1065e9a410cd3b620eb59df29896a8bdaea1bc5b5ca441c6c4e388b7`。各自服务身份与包内 manifest 核对，不要求跨平台 build ID 相等。

已通过：

- Windows 当前辅助程序的协议、安全、来源回归、快捷入口及离线 Skill 安装器 8 项检查；Linux 7 项通过、Windows 专属项跳过。Windows 测试需要独立 helper 布局，不能用含桌面宿主的目录测试“未安装桌面”分支。
- Windows 当前 CI 安装器解出的宿主启动、单实例、同配置并发、双配置隔离、退出隔离和 1380×900 窗口尺寸往返；Linux 当前 AppImage 的 extract-and-run 启动及同样并发/隔离/窗口往返。Linux 必须经过 AppRun 设置运行环境，直接运行内层 ELF 曾因缺少 WebKitNetworkProcess 失败，正确入口复验通过。
- 1,000 会话、100,000 条事件、12M 合成负载的单轮冷启动：Windows 4,576 ms、Linux WSL 文件系统 1,793 ms；5 次摘要中位数分别 0.5 / 0.6 ms。只作当前单轮观察，不扩展为性能保证。
- 两端当前宿主源码的隔离临时签名更新产品均严格通过六项：离线、下载中断、错误签名、版本不符、已签名但无效的安装包恢复，以及 1.0.1 → 1.0.2 安装/重启。设置、身份、导入快照、目录记录及 24K 保留，宿主退出码 0；Windows 测试产品卸载退出码 0。源码摘要均为 `dcf923bd8d29a4ae78fd91329c55a9a526c76a76c061640a0a79c30d0fbd4e35`。测试产物和生产 CI 包在不同 target 目录，未覆盖交付包。

另通过远程实际操作 Windows 原生窗口：

- 当前包界面与原生来源选择框切到中文/空格目录，服务 PID、端口改变，12K → 24K；CLI open 能激活原窗口。
- 原生完整及脱敏保存成功，解压分别 8,000 / 7,382 bytes；完整保留虚构消息和路径，脱敏去除二者。系统文件选择框导入脱敏快照后为 2 会话、48K。取消保存不写目录记录。
- 独立 CLI 服务连接时，界面恢复自动发现被明确拒绝，preferences.json 逐字节相同，原 PID 不变且仍提供 48K。退出宿主后独立服务仍存活，再通过其认证接口停止本轮服务。
- 监视进程的完整保存/导入重跑及独立服务拒绝检查，最终主动退出均为 0；本轮远端宿主与辅助程序已清理。

初次未捕获宿主退出码的 Windows 界面尝试中曾没有导出文件、随后宿主消失，原因未定位。原生对话框后来可复现拒绝正斜杠 Windows 文件名；改用系统目录和反斜杠后，监视进程的重跑全部通过，但这不证明首次宿主消失的根因已修复。没有据此改动产品或宣称已排除历史启动超时/WSLg 堆损坏风险。

证据保存在忽略目录 `output/verification/remote-windows-current/` 和 `remote-linux-current/`，包括协议日志、desktop-smoke/concurrency/performance/update-e2e 报告；Windows 另含 native-export-check.json、gui-import-check.json、independent-source-rejection.json、两次宿主退出报告及来源切换/拒绝截图。不会提交运行令牌、真实用户数据或密钥。

## 本轮稳定版准备范围

用户明确选择“稳定版准备，仅承诺已验收的环境”。首发候选仅列上述 Windows 11 x64 / 已有 WebView2 与 macOS 26.6.2 arm64；Linux WSLg 通过的检查作为实验性证据，Intel Mac、最低系统和独立 Linux 桌面不进入首发稳定支持范围。完整硬件与旧系统矩阵保留为后续工作。

发行组装器新增可重复 `--platform`，可以只组装 Windows x64 和 Mac arm64；默认仍要求四目标，选中目标缺包/缺签名、未知或重复平台均拒绝。先验证全部选中输入、拒绝非空输出目录，再通过排他锁保证同目录只有一个写入者。并发混包回归在加锁前失败、加锁后通过；独立复查确认解决。共 20 项路由、发行和更新清单测试通过；其中更新清单 6 项另外在 Windows 与 Linux 分别运行，均通过。

[发行步骤](desktop.md#更新与发行)给出限定平台命令、稳定/实验制品分离与发布前门槛。用户随后明确选择 Windows 和 Mac 免费直装发行：Windows 不购买应用签名证书，Mac 使用 ad-hoc 签名，Developer ID 和公证不作首发门槛。用户随后进一步要求首发启用自动更新。长期公钥与 GitHub Releases 独立 HTTPS 频道已配置，最终生产包的手动安装/升级证据及真实渠道进度见下节；尚未公开发布。

## 当前验证与边界

本机 Rust workspace 57 项测试、桌面目录记录 3 项测试、前端 49 项测试、Svelte 检查、workspace/桌面 all-feature clippy、格式检查及发布/更新清单 6 项测试已通过。新增来源回归在当前 arm64 辅助程序通过。

当前源码的云端矩阵：

- [Desktop clients 40064637199](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199)：四目标全部通过，包含生产打包、格式/all-feature clippy、目录记录及 workspace 测试、协议、安全、启动/并发和包校验。Mac 两架构均通过全部六项签名更新/恢复检查，升级后保留 24K 及目录记录；Intel 报告的宿主源码摘要与本地 `dcf923bd…` 一致，宿主退出码 0，经历 3 次导航前置拒绝后正常继续。
- [Native compatibility 40064637196](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637196)：四目标和候选组装全部通过，发布及公开下载步骤跳过。

收尾时保留了远端并行加入的 CI 共享构建、缓存及推送差异路由（7100acd、8b2d13d）；应用源码相对 d54921d 没有变化。本地 15 项路由、发行和更新清单测试通过；[统一 Client CI 40065344097](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40065344097) 的共享检查、Native 四目标、Desktop 四目标及候选组装也全部通过，正式发布步骤跳过。新的工作流检查分工见 [CI 说明](development/ci.md)。

最终发行工具提交 **f5de6a3** 的 [Client CI 40069212374](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40069212374) 已完整通过：共享检查（含 20 项路由/发行/更新清单测试）、Native 四目标、Desktop 四目标及候选组装均成功，发布和发布后下载步骤跳过。首次 Mac arm64 任务在编译和 ad-hoc 签名通过后，生成 DMG 时遇到 `hdiutil: create failed - Resource busy`；待整轮结束后仅重跑失败任务，DMG 及后续检查全部通过，其余成功任务没有重新执行。没有为该次资源占用错误修改产品代码。

已下载并校验该轮 Mac 两架构的验证 ZIP，逐项核对严格更新报告：均为当前宿主源码摘要 `dcf923bd…`、六项通过、升级后 24K 与目录记录保留，各有 2 次导航前置重试。证据位于本地 `output/verification/pr9-final-intel-verification.zip` 和 `pr9-final-arm-verification.zip`，ZIP SHA256 分别为 `d612ada4534516ef95219c0c8eeb7792201e34d19da48d7be128b032fdab271e` / `09fcbafec87ea2787f2cd2c99aec9d24850a7b2cb8d89478fa5ab5e21a940a39`。后续收尾仅修改 README 与文档，应用和发行工具源码未变。

当前源码 d54921d 的桌面开发制品已生成，ZIP 摘要来自对应 Actions artifact 元数据；包内文件摘要另见各自的 artifacts.json：

| 目标 | 下载 | ZIP SHA256 |
| --- | --- | --- |
| Windows x64 | [NSIS](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199/artifacts/12549167391) | `dfed6f81d110d2e7745fa7ad7308585dceaf077fb07940220b5697a644254811` |
| macOS arm64 | [app / dmg](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199/artifacts/12549390279) | `76ee7626137c6a9e93354b19f8fbe4137366626f413eabedcd3c51188b56cbb1` |
| macOS x64 | [app / dmg](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199/artifacts/12549043948) | `a425e8ceb9618196a63776c1d9c9164147c3afb19d88fc14cab6d52d93f95688` |
| Linux x64 | [AppImage / deb](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40064637199/artifacts/12549361635) | `9df783681d192bff1972e930a5223fb587d3ca153f8dfc837168e0d7cb563168` |

更早 Windows/Linux 的 GUI、安装和大快照证据仍见 [原验证记录](desktop-verification.md)，属于对应历史源码。新改动不能只凭旧包结果宣称跨平台验证完成。

仍需独立环境或外部资源的项目：Intel Mac GUI、最低 macOS 14.5、Finder 安装/移除与 Dock 菜单完整矩阵；Windows 10/全新或离线 WebView2；独立 Linux 桌面及无托盘交互；真实登录、多显示器、高 DPI 切换、睡眠唤醒及长期运行；极限快照规模和完整提示框性能矩阵。付费 Windows 签名及 Mac Developer ID/公证列为可选改进；正式自动更新渠道已配置，实际公开 HTTPS 更新验收仍待完成。保留此前未定位的 Windows 启动超时和 WSLg 堆损坏风险，不以本轮通过宣称根因已经修复。

PR 保持草稿；没有合并、打发布标签或正式发布。


## 免费直装最终候选与 HTTPS 渠道

应用变更 **051b5b9** 修正未配置自动更新时的提示，避免将正常直装包误称为开发包；**dccc59d** 加入正式更新公钥及 HTTPS 配置。该提交的 [Client CI 40080145122](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40080145122) 全部通过：共享检查、Native 四目标、Desktop 四目标和候选组装成功；Mac 两架构的签名更新/恢复通过，公开发布和发布后下载步骤跳过。此处 CI 的隔离更新产品证据与下述真实产品验收分别记录。

最终候选使用生产 profile、真实产品标识 `io.github.luyh7.cousash`，没有启用 `desktop-tests`。Windows 为未购买系统签名的 NSIS；Mac app/helper 的完整 ad-hoc 签名验证通过。更新签名使用长期密钥，私钥/口令不进入仓库、远端 Windows 或制品。两端更新文件均已用仓库公钥做密码学验证，签名绑定 `version:1.0.1`，篡改字节被拒绝。

| 最终候选文件 | SHA256 |
| --- | --- |
| `cousash-1.0.1-windows-x64.exe` | `88f83391937882397fb41dc92496a03fec66eeda152ddf115721646a7cb5ea74` |
| `cousash-1.0.1-darwin-arm64.dmg` | `2ad0b9b6e2c3baab905ce74a32ed3cae775f1877188dd673c6c8a93eb2bd0e93` |
| `cousash-1.0.1-darwin-arm64.app.tar.gz` | `862ce1e1c16556c0b633b01029fd70626d382925196ff2089aea34c1246670f2` |

Windows 随包服务 build ID 为 `711fc396b682fb07d46b2e72a87afc60d24d4cc6216e1f8d77c84fae8fd9fa49`；Mac 为 `8eec4d84bfe740c4db51d7853cf20598482adb222ce37f1c353a85231ee474c6`。与上文历史包摘要分别对应，不能混用。

在上述 Windows 11 / 已有 WebView2 与 Mac arm64 实机，使用隔离合成数据、中文/空格安装目录和同产品标识的内部 1.0.0 基线，两端均已通过：全新安装 1.0.1、同版本重装、卸载保留业务数据、卸载后安装旧完整包回退，以及 1.0.0 → 1.0.1 完整包覆盖升级。Windows 覆盖升级时旧宿主正在运行，安装器正常停止本轮宿主及随包服务，捕获的退出码均为 0。升级后准确的 24,000 tokens、语言/时区与来源设置、设备身份、导入快照、目录记录和窗口尺寸保留。原有独立用户服务未停止。两端实际原生界面均已检查并保存截图。

最终 7 项候选资产（两端安装/更新文件、更新签名、`latest.json` 与 `SHA256SUMS`）已上传 [Release 草稿](https://github.com/luyh7/codex-usage-dashboard/releases/tag/untagged-ad4497152307866b46dd)，GitHub 资产摘要与本地文件一致。固定更新入口为 `https://github.com/luyh7/codex-usage-dashboard/releases/download/desktop-updates/latest.json`，清单仅列 Windows x64 与 Mac arm64。草稿不能匿名下载；需明确允许公开为预发布后，才能完成真实 HTTPS 发现、下载、签名验证、安装/重启和数据保留验收。当前两端已准备 1.0.0 基线，未将这一步记录为通过。浏览器下载触发的 Gatekeeper/SmartScreen 首次打开体验也仍需验证；本地复制或 CLI 下载不替代该证据。

本地证据位于忽略目录 `output/free-release/`：`acceptance-mac-auto/progress.json`、从 Windows 复制的 `windows-acceptance-progress.json`、两端 `*-configured-installed-24k.png`、发行清单及签名验证工具。运行令牌、合成日志、签名私钥和口令均不提交。PR 保持草稿，Release 保持草稿，没有合并或公开候选。
