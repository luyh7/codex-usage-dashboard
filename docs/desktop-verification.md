# 桌面验证记录

Windows、macOS 两架构和 Linux 开发包均已在各自系统构建、启动；完整三端交互验收尚未完成。Windows/Linux 有本地图形验证，macOS 有 GitHub Actions 运行证据。用户已确认没有 Mac 实机，实机交互检查保留为未验证；构建和启动不代替该检查。草稿 [PR #9](https://github.com/luyh7/codex-usage-dashboard/pull/9) 包含实现、验证脚本和四目标流水线。

## 基线与制品

- 实施前已 fetch，基于 origin/main：38b9d031139900998cc3a8797b5d42513ad078aa。
- 分支：codex/cross-platform-desktop。
- worktree：C:\Users\luyh7\.codex\worktrees\cross-platform-desktop\codex-usage-dashboard。
- 原计划 worktree 属于另一 chat，附加被拒绝；该目录已快进且计划保留，后续实现全部在当前独立 worktree。
- 2026-10-24 用户已授权提交、推送本任务分支并创建草稿 PR，执行云端构建验证；不包含合并、正式发布或购买证书。
- 开发包版本：1.0.1。使用、构建和更新说明见 [桌面客户端](desktop.md)。

| 目标 | 构建/制品 | 真实 GUI/核心功能 | 安装/移除 | 更新/恢复 | 应用签名/公证 |
| --- | --- | --- | --- | --- | --- |
| Windows x64 | NSIS 已构建 | WebView2 看板、详情、设置、CLI 聚焦、导入、完整导出及取消、窄窗口键盘提示框 | 中文/空格目录、开始菜单、覆盖、移除及数据保留通过 | 隔离签名测试产品升级与失败恢复通过 | NotSigned，无正式证书 |
| macOS arm64 | macOS 15 CI 生成 app/dmg | 宿主启动、重复/并发启动、隔离配置、窗口状态及 CLI 通过；人工 GUI 未验证 | app 解包运行；Applications/Finder 安装移除未验证 | 空 URL 崩溃修复后，严格六项更新/恢复检查通过 | ad-hoc 完整性校验通过；无 Developer ID/公证 |
| macOS x64 | macOS 15 Intel CI 生成 app/dmg | 宿主启动、重复/并发启动、隔离配置、窗口状态及 CLI 通过；人工 GUI 未验证 | app 解包运行；Applications/Finder 安装移除未验证 | 空 URL 崩溃修复后，严格六项更新/恢复检查通过 | ad-hoc 完整性校验通过；无 Developer ID/公证 |
| Linux AppImage x64 | Ubuntu 22.04 CI 基线包及 Ubuntu 26.04 本地包 | WSLg Wayland 真实 WebKit 看板、详情、设置、键盘提示、原生导出和快照导入；Ubuntu 22.04 Xvfb 启动/并发通过 | extract-and-run 与实际 FUSE 挂载均验证 | 隔离签名测试产品升级与失败恢复通过 | 未配置正式更新源/发行签名 |
| Linux .deb x64 | Ubuntu 22.04 CI 基线包及 Ubuntu 26.04 本地包 | 安装后的真实宿主与服务烟测；Debian 12 用户环境 Xvfb 运行通过 | 两环境安装、重装、移除及数据保留通过 | 包渠道重装已验证；应用内为包管理器提示 | 未配置正式包仓库签名 |

安装包相对本 worktree：

- desktop/target/x86_64-pc-windows-msvc/release/bundle/nsis/Codex Usage Dashboard_1.0.1_x64-setup.exe
- desktop/target/linux/x86_64-unknown-linux-gnu/release/bundle/appimage/Codex Usage Dashboard_1.0.1_amd64.AppImage
- desktop/target/linux/x86_64-unknown-linux-gnu/release/bundle/deb/Codex Usage Dashboard_1.0.1_amd64.deb

当前应用源码 a78cfc4 的 CI 制品可直接下载，均为开发包：

| 目标 | 下载内容 |
| --- | --- |
| Windows x64 | [NSIS](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007970892/artifacts/12526440061) |
| macOS arm64 | [app / dmg](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007970892/artifacts/12524958210) |
| macOS x64 | [app / dmg](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007970892/artifacts/12525742049) |
| Linux x64，Ubuntu 22.04 构建基线 | [AppImage / deb](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007970892/artifacts/12524584684) |

两份 macOS ZIP 已下载到本机隔离缓存并对照 GitHub 的 ZIP 摘要，包内宿主/辅助程序的 Mach-O 架构、版本清单及 DMG 摘要均已检查，生产包不含测试控制接口。arm64 DMG SHA256 为 `4e126a55c63fdea34b558a9fa9cfeb97a5ad345344917f3fdb92da1bdb0b9227`，x64 为 `4d826ff596f4a8ea25272c34222138223b98f42f96f67308703133b57d3f9dfe`；见 macos-package-aarch64-apple-darwin.json、macos-package-x86_64-apple-darwin.json。静态校验不算 Mac 实机验收，应用签名检查由对应 macOS CI 执行。

对应 release/artifacts.json 记录实际文件、大小和 SHA256。最终复核文件为未提交的 output/verification/final-windows.json、final-linux22.json：检查当前健康接口、当前 worktree 中对应随包程序的 --build-info、实际辅助程序身份和安装包 SHA256。Ubuntu 26.04 本地包的上一轮记录保留在 final-linux.json。AppImage 程序还逐字节对照最终 AppDir；打包器处理后的 ELF 不与 Cargo 原始文件直接混比。生产宿主不含 desktop-tests 控制接口。

## 环境与自动化结果

Windows：11 Pro 10.0.22631，x64，WebView2 155.0.4283.62。Linux：WSLg Ubuntu 26.04，x64，内核 6.18.40.1-microsoft-standard-WSL2，WebKitGTK 2.52.6。Rust 1.96；Windows Node 26、Linux Node 24。这些环境不能代替更旧系统或独立桌面的支持证明。

已通过：

- Windows/Linux Rust workspace 57 项测试、clippy；桌面 crate 两端全部 feature 的 clippy；Rust 格式检查。
- 前端 check 0 错误、0 警告，49 项测试；既有发布组装 5 项与更新清单架构隔离测试。
- 协议/安全/兼容集成：Windows 6 项；Linux 5 项通过、Windows 快捷方式用例跳过。
- 并发 CLI 复用索引、损坏和伪造记录保护、设置写入失败及部分保存、时区日期边界、完整/脱敏快照、失败保留目标文件。
- 原生宿主重复启动、正常退出及退出时保存窗口状态；Unix 控制目录 700、记录 600。
- Windows/Linux 同配置同时冷启动两个进程，只保留一个宿主，另一进程正常返回；两个配置同时运行，退出一个不影响另一个。修复了并发启动连接成功后仍弹锁错误的问题。
- 退出、重开、再次退出后窗口尺寸保持一致。Windows 原生菜单曾导致每次重开高度减少 20 像素，现已在菜单安装后恢复目标客户区尺寸。Linux 的 configure-event 尺寸含 GTK 窗口阴影，曾导致宽度逐次增加；现从 GTK 主线程读取客户区尺寸。两端回归均确认 1380×900 保持不变。
- Windows 服务退出后恢复为新 PID；宿主崩溃后自己的辅助程序退出；其他 worktree 服务保留。
- Windows/Linux 覆盖或重装后设置与导入文件 SHA256、设备身份不变；移除后仍能查询保留的 24K 虚构用量。

[Native CI 40007971023](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007971023) 的 Windows、macOS arm64/x64、Linux 四目标及发行候选组装全部通过；正式发布步骤跳过。[Desktop CI 40007970892](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007970892) 四目标全部通过，包含前端检查和测试、打包、格式/all-feature clippy、workspace 测试、协议、宿主启动及并发检查；两个 Mac 目标还通过包资源、ad-hoc 签名和严格六项更新/恢复检查。两条流水线验证的应用源码均为 a78cfc4，包含下述空 URL 与大存档写入修复；最后的文档提交不改变应用源码。

首次使用空的 `sessions/` 目录时，原接口仅凭目录存在报告来源可用。现按实际索引结果报告空目录，且首次无会话时自动打开既有设置页；Windows/WebView2 与 WSLg/WebKit 均实际看到空来源提示，Windows 来源选择取消不改变设置。回归集成测试同时覆盖此状态。证据：windows-first-run-empty.png、linux-first-run-empty.png。

此前 0eac97a 的 Ubuntu 22.04 CI FUSE AppImage 已实际中断自有服务并恢复：约 3.14 秒后出现新服务 PID/端口，宿主 PID、设置和导入文件哈希保持，统计仍为 24K；见 linux22-service-recovery.json。最终实例已换为 a78cfc4 的包并重新核对身份，恢复时间不冒充该新包的独立测量。

Ubuntu 22.04 CI 制品已验证 ZIP SHA256 和包 SHA256，随后在 WSLg Ubuntu 26.04 上用 FUSE 运行，实际进程路径位于 `/tmp/.mount_…`；真实 WebKit 界面显示 24K 测试用量，见 ci-linux-provenance.json、final-linux22.json、linux22-fuse-dashboard.jpg。Debian 12 是 WSL 内核上的隔离用户环境，glibc 2.36、GTK 3.24.38、WebKitGTK 2.50.6；以 UID 1000 在 Xvfb 中运行同一 Ubuntu 22.04 `.deb`，通过宿主烟测、协议检查及重装/移除后 24K 用量、设置、设备身份和导入文件保留。见 debian12-installed-smoke.json、debian12-retention.json。这不等于独立 Debian 桌面的完整人工验收。

曾出现一次 Windows CLI 冷启动超时和一次宿主烟测超时，确切原因尚未定位。随后隔离 CLI 冷启动、安装后启动、覆盖后启动均通过；10 个全新状态目录的连续宿主冷启动也全部通过，约 0.85–3.10 秒。证据：windows-cold-starts.json。后续通过不代表已经证实前两次超时的根因。

2026-10-24 延续验证发现一个 WSLg AppImage 实例已退出，日志尾部有 `free(): corrupted unsorted chunks`，触发原因未确定。日志保留为 linux-appimage-heap-crash.log；同包连续三次受控退出返回 0 且无该错误，不能据此宣布根因已消除。Linux 对自有服务发送 SIGTERM 后约 5.6 秒恢复为新 PID，宿主与业务数据保留，见 linux-service-recovery.json。Windows 24K 小夹具的 30 秒空闲采样，9 个宿主/服务/WebView 进程 CPU 增量为 0，私有内存约 268 MiB；这不是长期或大负载指标。Linux 采样期间子进程集合发生变化，不据此宣称完整空闲资源通过。

真实界面检查修复了 AppImage 保存框默认进入临时解包目录的问题；现默认文档目录，不存在时用用户主目录。新包已实测：Windows 打开文档目录并取消；Linux 打开用户主目录，导出到中文/空格路径，解压后确认一个会话、12K 本机用量，并在真实 WebKit 文件选择器中导入 Windows 快照，合计仍为 24K。日志来源选择从用户主目录开始。

## 更新与恢复

tools/desktop-update-e2e.mjs 使用独立产品标识、临时密钥、loopback 源和 desktop-tests feature，实际执行 1.0.1 → 1.0.2。Windows 与 Linux AppImage 均通过：

1. 离线源、下载中断、密码学签名损坏、清单版本与签名版本不符均被拒绝，原服务继续运行。
2. 签名有效但安装格式无效时，旧应用文件不变，服务恢复，安装前窗口状态已保存。
3. 正确制品安装并重启；版本和辅助程序匹配；设置、设备身份、导入快照保留，统计仍为 24K。

证据：desktop-update-e2e-win32.json、desktop-update-e2e-linux.json。最后回归的升级发起端使用当前宿主源码，两个报告的 baseline_source_sha256 与当前 main.rs、native.rs 内容哈希相同；目标是冻结的已签名 1.0.2 测试制品。测试密钥、运行文件和测试产品未加入 Git，测试权限未授予生产 WebView。正式开发包没有配置更新公钥/端点，界面明确显示未配置渠道。临时更新签名不等于应用签名或公证。

macOS 复验曾捕获 Wry 0.57.1 对导航期间空 WKWebView.URL 执行 unwrap 的主线程 panic；原测试仅断言“存在错误”可能误认该错误为签名校验失败，现已要求错误确实对应签名、版本或网络原因，并保存宿主输出和退出状态。客户端改为在 UI 线程安全读取可空 URL，无有效页面时拒绝原生命令，仍要求精确服务来源。驱动仅重试明确的 untrusted desktop page 导航前置拒绝，持久拒绝会超时失败，其余错误仍严格断言。最终 a78cfc4 在 arm64 与 Intel 均通过全部六项测试，宿主退出码均为 0；分别等待了 3 次、1 次导航前置拒绝，正确重启到 1.0.2，保留设置、设备身份和导入数据，统计仍为 24K。报告见 ci-macos-arm64-final、ci-macos-intel-final 中的 desktop-update-e2e-darwin.json；对应 [arm64 验证制品](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007970892/artifacts/12525791514)、[Intel 验证制品](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007970892/artifacts/12526721659)。这不代替 Mac 人工 GUI 验收。

Windows 安装器启动后的异步失败、安装中途磁盘故障及正式更新源仍需验证。恢复策略是保留业务目录、重新打开旧应用或重新安装完整版本，不宣称所有回退路径均已验证。

## 性能与证据

合成负载：1,000 会话、100,000 条用量事件、12M tokens。Windows 冷启动约 5.70 秒、摘要中位数 1.6 ms；Linux 本机文件系统约 2.01 秒、1.4 ms。WSL 共享 C 盘的一次并行负载测试启动约 45.9 秒；文件系统和负载条件不同，不作直接跨系统比较。这不是大快照、长期运行或空闲资源的完整性能验收。

进一步的真实 Windows/WebView2 与 Linux/WebKit 原生保存框验证使用 1,000 会话、100,000 条事件和 12M tokens：完整快照压缩约 2.5 MB、解压约 49 MB，脱敏快照解压约 47 MB，中文/空格路径保存后逐项核对会话、事件和统计；脱敏结果不含合成提示词、源目录或完整项目路径。双方导出的完整快照已通过系统文件选择框互相导入，合计 24M；脱敏导入也单独验证为本机加一份远端，共 2,000 会话、24M，关闭重开后保留。完整与脱敏导出的设备标识不同，同时导入会作为两份设备统计，测试中通过界面移除了重复的合成数据副本。证据见 large-native-*.json、large-import-*.json 和 large-linux-import-complete.jpg。

该测试发现远端存档保存使用未缓冲 JSON 写入，约 85 MB 的存档重命名请求在 Windows I 盘耗时 34,645 ms。改用 256 KiB BufWriter，并在原有文件 sync/原子替换前显式 flush，同一存档复测为 2,228 ms（约 15.5 倍改善）；快照内容哈希不变，统计保持 24M。见 large-remote-write-before-buffering.json、large-remote-write-after-buffering.json。两次是同一机器/数据的单次测量，不作为所有磁盘或大文件的普遍性能承诺。包含该修复的四目标 Native/Desktop 流水线均已通过；Windows/Linux 最新本地包已实际重开大数据配置并核对保留结果，最新 Ubuntu 22.04 CI deb 亦完成 Debian 12 重装/移除复验。

当前 Linux AppImage 的另一次 30 秒空闲采样中，4 个宿主/服务/WebKit 进程及启动时间保持不变，CPU 共 0.05 秒（单核约 0.17%），RSS 合计约 736 MiB；见 desktop-idle-linux.json。此前进程集合变化的采样仍不算通过，此次也只覆盖 24K 小夹具。连续日志追加测试在 133 秒内执行 8 次写入，SSE 读取 41 次，用量由 12,000 增至 12,960，各次约 1.0–1.25 秒反映到摘要，宿主及服务 PID 保持；见 linux-live-log-growth.json，该轮 build ID 为 022fe586。

本机 C 盘空间不足曾导致 NSIS 临时文件构建失败及一次 Linux 更新重启等待失败；保留失败日志，将本任务测试安装目录和构建临时目录移至空闲盘后，当前宿主源码的 Windows、Linux Wayland 和 X11 六项更新检查均通过。该结果没有把磁盘耗尽时的所有安装故障恢复列为已验证，也不能用于解释上面独立记录的堆错误。

未提交的 output/verification/ 保存：

- windows-install.json、linux-package.json：安装、移除和数据保留。
- desktop-performance-win32-x64.json、desktop-performance-linux-x64.json：合成负载。
- appimage-x11.json、appimage-wayland.json：WSLg 启动模式。
- desktop-concurrency-win32.json、desktop-concurrency-linux.json：并发冷启动与配置隔离。
- final-native-files.json、linux-appimage-native-export-import.png：最后原生文件操作回归。
- windows-export-tooltip.png：先前轮次导出后已打开的提示框。
- windows-narrow-keyboard-tooltip.png：约 800 像素宽窗口中的已打开键盘提示框。
- linux-appimage-wayland-details.png、linux-webkit-keyboard-tooltip.png：实际 AppImage/WebKit 详情与已打开提示框。
- final-windows-dashboard.jpg、linux22-fuse-dashboard.jpg：最后一次本地构建与当前 Ubuntu 22.04 CI 制品对应的真实看板，两端显示相同的 24K 虚构用量。

截图属于对应图形轮次，最终二进制身份以最后复核 JSON 为准。

2026-10-24 最后复核的隔离实例（虚构日志与导入快照，合计 24K）：Windows http://127.0.0.1:14333/?period=all，Linux/WSLg FUSE AppImage http://127.0.0.1:46363/?period=all。Windows build ID 为 `e5433151949aeff601fe56202e846c1263103d3d9a99a5fcd9abb8ed9a8b7313`；Linux 为 `a24eac4c3ce62accb5acd7f5f71a37742ba98afbb476499aea7239641a9d1971`。Linux 使用应用源码 a78cfc4 的 Ubuntu 22.04 CI 制品，随包辅助程序复制到本工作树 `output/verification/codex-usage-dashboard-linux22`，其 `--build-info` 与健康接口相同；实际宿主和辅助程序均与该制品 AppDir 逐字节匹配。Windows 同样核对本工作树的二进制和包。最终文档提交不改变应用源码。地址仅在对应验收实例仍运行时有效。Ubuntu 26.04 本地包的 `26610f5c…` 大数据验证实例已主动退出，最终地址采用较低构建基线的 CI 包。

当前工作树安装包 SHA256：Windows NSIS `c884a9d1e87922e870a6acabdfd80386977fe43c9d0aee36f732ea4057e10da9`；Ubuntu 26.04 本地 AppImage `360b6a30c0d55f94fa63d64160a04d04361158de3ff69ae16bbdb050bc7db0fe`、`.deb` `dd8f60c14bd863037d8e79cc00dfdc36c5ce7f17df5fabb86841243e5f663531`。实际文件与各平台 artifacts.json 已逐字节校验。较低基线的 Ubuntu 22.04 CI AppImage 为 `990ff6e80406566eddd5e8e957e17c505f10f731e67ac7a8febb0fb1d888dddc`、`.deb` 为 `e5c7a1f9aa5c196164f726c8502aec76f09eefdb162b520752cebc3b263c11f7`；来自 [a78cfc4 的 Linux 制品](https://github.com/luyh7/codex-usage-dashboard/actions/runs/40007970892/artifacts/12524584684)，包内宿主和辅助程序、ZIP 摘要、包摘要均已核对，该最新 `.deb` 已重新完成上述 Debian 12 检查。

## 验收清单对照

按计划中的顺序逐条对照；“部分”表示该条仍有未覆盖的条件，不能作为整条通过。

| 序号 | 要求 | 当前证据与剩余范围 |
| --- | --- | --- |
| 基础 1 | 四目标构建、运行及运行库 | 四目标原型及随包辅助程序已构建运行，用户入口不调用开发工具，运行库策略已提供；全新用户安装验收另见日常 1 |
| 基础 2 | 真实 Svelte 与当前 build ID | Windows/WebView2、Linux/WebKit 实际界面及随包身份通过；Mac 为宿主/服务 CI，人工界面未验证 |
| 基础 3 | 8765 占用时安全选端口 | 本地其他实例保留并使用动态端口；Mac 未做独立的 8765 占用图形场景 |
| 基础 4 | 并发单实例与配置隔离 | 四目标当前修复版 CI 均已通过同配置并发、双配置及退出隔离 |
| 基础 5 | 记录损坏、身份冲突、超时及索引失败 | 运行协议测试覆盖损坏/伪造记录、未知监听者和身份冲突；全部故障的三端界面反馈矩阵未完成 |
| 基础 6 | open/status/summary/export | 四目标协议与统计/快照通过；Windows 实际聚焦通过，Mac 的视觉聚焦未验证 |
| 基础 7 | JSON、错误码与退出码 | 四目标 Native CI 和协议/安全测试通过，控制令牌不进入命令 JSON |
| 基础 8 | CLI 复用已有索引 | 四目标协议测试及既有原生兼容验证通过 |
| 基础 9 | 关闭重开与归属隔离 | 并发/退出与窗口状态 CI 通过；Mac Dock/系统关闭交互未验证 |
| 基础 10 | 三端筛选、详情、SSE、导入导出 | Windows 与 WSLg WebKit 主要流程通过；Mac 交互未验证 |
| 基础 11 | 设备数据布局及全部按钮状态 | 保留当前布局、无强制刷新按钮；Windows/Linux 主要文件流程通过，三端全部状态未完整验收 |
| 基础 12 | 提示框完整矩阵 | 已有共享单元测试和 Windows/Linux 窄窗口、键盘、已打开提示截图；Mac 及全部模态场景未验证 |
| 基础 13 | 语言/时区持久化与无额外时区标签 | 四目标设置/日期协议测试、Windows/Linux 界面通过；Mac 视觉检查未验证 |
| 基础 14 | 旧 CLI、浏览器及数据安全兼容 | 四目标 Native CI、协议/HTTP 安全和安装器兼容测试通过 |
| 基础 15 | 开发、使用及边界文档 | desktop.md、本文和计划已提供 |
| 基础 16 | 最终健康身份与实际 URL | Windows、当前 Ubuntu 22.04 CI 包本地实例均已核对并提供地址；Mac CI 实例按测试退出，没有可持续访问的 Mac 实例 |
| 日常 1 | 无开发工具用户安装与系统入口 | Windows、Ubuntu/WSLg、Debian 用户环境已有包检查；Mac Applications/Dock 和全新用户机器未验证 |
| 日常 2 | 自动发现、空/无权限/自定义来源 | 协议及 Windows/Linux 首次空来源实际界面通过；Mac 文件选择和完整权限交互未验证 |
| 日常 3 | 中文路径、原生文件、取消覆盖、大快照 | Windows/Linux 中文/空格路径、原生选择、取消及约 49 MB 完整/47 MB 脱敏快照往返通过；Mac 文件界面及更大边界规模未验 |
| 日常 4 | 托盘、关闭、登录启动与退出清理 | 按平台实现、拥有者退出和隔离 CI 通过；真实登录启动及 Mac Dock 未验证 |
| 日常 5 | 窗口、多屏、高 DPI、键盘与响应 | 窗口尺寸往返、Windows/Linux 键盘及基础性能已有证据；多屏/高 DPI 硬件未验证 |
| 日常 6 | 代表性启动、交互与空闲性能 | Windows/Linux 12M 合成负载与小夹具空闲采样已记录；Mac 和完整交互性能矩阵未完成 |
| 日常 7 | 睡眠、服务退出及长期恢复 | Windows/Linux 自有服务恢复及日志持续追加通过；实际睡眠唤醒和长期运行未验证，已保留异常风险 |
| 日常 8 | Mac Dock/Cmd+Q、Linux X11/Wayland/无托盘 | Mac 无实机；Linux WSLg、Xvfb 及 X11/Wayland 更新测试通过，独立桌面完整交互未验证 |
| 日常 9 | 三端同夹具、路径权限及设备身份 | 四目标同夹具 12K、Unix 私有运行目录与协议测试通过；完整平台硬件矩阵仍受上述环境限制 |
| 更新 1 | 四目标更新与 deb 包渠道 | Windows、Linux AppImage 和 Mac 两架构严格六项均通过；最新 deb 已重装/移除并保留数据，应用内提示包渠道 |
| 更新 2 | 网络/签名失败与安装恢复 | 当前测试覆盖四类拒绝及同步无效安装恢复；异步安装器失败、磁盘故障和所有回退路径未验证 |
| 更新 3 | 升级/卸载保留设置、身份、快照 | Windows、Ubuntu/WSLg、Debian 通过；Mac 两架构升级保留通过，Finder 默认移除待验 |
| 更新 4 | 可重复构建、签名/公证结果分列 | 四目标锁定依赖和 CI 已接入，ad-hoc 与临时更新签名有证据；缺正式证书/公证/发布源已单列 |
| 更新 5 | 制品、路径、校验、说明及 Agensi 可选 | Windows/Linux 包和最终 CI 四目标包、校验与说明已提供；Agensi 没有成为任何客户端依赖 |

## 未验证项

- macOS 实机 WKWebView 视觉/交互、Applications/Finder 安装移除、Dock、Cmd+Q、菜单、原生文件及最低 14.5 系统；用户已确认没有实机并保留未验证。云端更新修复复验结果另列，不能代替以上项目。
- Windows 10、无 WebView2 的全新机器和离线运行库安装。
- 独立 Linux X11/Wayland 和无托盘桌面的完整交互；Ubuntu 22.04 CI、Debian 12 用户环境和 WSLg 的已验证范围见上文。
- 各端全部提示框位置、层级、鼠标、键盘和模态场景；单元测试及部分截图不能替代完整矩阵。
- 真实登录启动、多显示器切换、高 DPI、睡眠唤醒、长期运行；Mac 大快照、更大文件边界和完整性能测量。Windows/Linux 约 49 MB 的已验证范围见上文。
- 正式 Windows 证书、macOS Developer ID/公证及正式更新签名/发布资源。

Agensi 未接入，也没有成为客户端、数据或更新依赖。开发包独立可用；完整跨平台验收和正式发行就绪仍须分别完成。
