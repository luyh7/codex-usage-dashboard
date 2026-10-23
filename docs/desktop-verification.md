# 桌面验证记录

Windows、Linux 开发包已实现并完成下列验证；完整三端验收尚未完成。用户没有 Mac 实机，已授权通过草稿 PR #9 执行新的 GitHub Actions 四目标矩阵。云端结果补齐中，构建及启动不能作为交互实机通过的证据。

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
| macOS arm64 | 脚本/CI 已接入，未执行 | WKWebView、Dock、菜单、文件操作未验证 | 未验证 | 未验证 | 无签名/公证结果 |
| macOS x64 | 脚本/CI 已接入，未执行 | WKWebView、Dock、菜单、文件操作未验证 | 未验证 | 未验证 | 无签名/公证结果 |
| Linux AppImage x64 | Ubuntu 26.04 开发包已构建 | WSLg Wayland 真实 WebKit 看板、详情、设置、键盘提示、原生导出和快照导入；X11/Wayland 启动烟测 | extract-and-run 已运行，FUSE 启动未验证 | 隔离签名测试产品升级与失败恢复通过 | 未配置正式更新源/发行签名 |
| Linux .deb x64 | Ubuntu 26.04 开发包已构建 | 安装后的真实宿主与服务烟测 | 安装、桌面入口、重装、移除及数据保留通过 | 包渠道重装已验证；应用内为包管理器提示 | 未配置正式包仓库签名 |

安装包相对本 worktree：

- desktop/target/x86_64-pc-windows-msvc/release/bundle/nsis/Codex Usage Dashboard_1.0.1_x64-setup.exe
- desktop/target/linux/x86_64-unknown-linux-gnu/release/bundle/appimage/Codex Usage Dashboard_1.0.1_amd64.AppImage
- desktop/target/linux/x86_64-unknown-linux-gnu/release/bundle/deb/Codex Usage Dashboard_1.0.1_amd64.deb

对应 release/artifacts.json 记录实际文件、大小和 SHA256。最终复核文件为未提交的 output/verification/final-windows.json、final-linux.json：检查当前健康接口、当前 worktree 的 --build-info、实际随包辅助程序身份和安装包 SHA256。AppImage 程序还逐字节对照最终 AppDir；打包器处理后的 ELF 不与 Cargo 原始文件直接混比。生产宿主不含 desktop-tests 控制接口。

## 环境与自动化结果

Windows：11 Pro 10.0.22631，x64，WebView2 155.0.4283.62。Linux：WSLg Ubuntu 26.04，x64，内核 6.18.40.1-microsoft-standard-WSL2，WebKitGTK 2.52.6。Rust 1.96；Windows Node 26、Linux Node 24。这些环境不能代替更旧系统或独立桌面的支持证明。

已通过：

- Windows/Linux Rust workspace 57 项测试、clippy；桌面 crate 两端全部 feature 的 clippy；Rust 格式检查。
- 前端 check 0 错误、0 警告，49 项测试；既有发布组装 5 项与更新清单架构隔离测试。
- 协议/安全/兼容集成：Windows 5 项；Linux 4 项通过、Windows 快捷方式用例跳过。
- 并发 CLI 复用索引、损坏和伪造记录保护、设置写入失败及部分保存、时区日期边界、完整/脱敏快照、失败保留目标文件。
- 原生宿主重复启动、正常退出及退出时保存窗口状态；Unix 控制目录 700、记录 600。
- Windows/Linux 同配置同时冷启动两个进程，只保留一个宿主，另一进程正常返回；两个配置同时运行，退出一个不影响另一个。修复了并发启动连接成功后仍弹锁错误的问题。
- 退出、重开、再次退出后窗口尺寸保持一致。Windows 原生菜单曾导致每次重开高度减少 20 像素，现已在菜单安装后恢复目标客户区尺寸。Linux 的 configure-event 尺寸含 GTK 窗口阴影，曾导致宽度逐次增加；现从 GTK 主线程读取客户区尺寸。两端回归均确认 1380×900 保持不变。
- Windows 服务退出后恢复为新 PID；宿主崩溃后自己的辅助程序退出；其他 worktree 服务保留。
- Windows/Linux 覆盖或重装后设置与导入文件 SHA256、设备身份不变；移除后仍能查询保留的 24K 虚构用量。

曾出现一次 Windows CLI 冷启动超时和一次宿主烟测超时，确切原因尚未定位。随后隔离 CLI 冷启动、安装后启动、覆盖后启动均通过；10 个全新状态目录的连续宿主冷启动也全部通过，约 0.85–3.10 秒。证据：windows-cold-starts.json。后续通过不代表已经证实前两次超时的根因。

2026-10-24 延续验证发现一个 WSLg AppImage 实例已退出，日志尾部有 `free(): corrupted unsorted chunks`，触发原因未确定。日志保留为 linux-appimage-heap-crash.log；同包连续三次受控退出返回 0 且无该错误，不能据此宣布根因已消除。Linux 对自有服务发送 SIGTERM 后约 5.6 秒恢复为新 PID，宿主与业务数据保留，见 linux-service-recovery.json。Windows 24K 小夹具的 30 秒空闲采样，9 个宿主/服务/WebView 进程 CPU 增量为 0，私有内存约 268 MiB；这不是长期或大负载指标。Linux 采样期间子进程集合发生变化，不据此宣称完整空闲资源通过。

真实界面检查修复了 AppImage 保存框默认进入临时解包目录的问题；现默认文档目录，不存在时用用户主目录。新包已实测：Windows 打开文档目录并取消；Linux 打开用户主目录，导出到中文/空格路径，解压后确认一个会话、12K 本机用量，并在真实 WebKit 文件选择器中导入 Windows 快照，合计仍为 24K。日志来源选择从用户主目录开始。

## 更新与恢复

tools/desktop-update-e2e.mjs 使用独立产品标识、临时密钥、loopback 源和 desktop-tests feature，实际执行 1.0.1 → 1.0.2。Windows 与 Linux AppImage 均通过：

1. 离线源、下载中断、密码学签名损坏、清单版本与签名版本不符均被拒绝，原服务继续运行。
2. 签名有效但安装格式无效时，旧应用文件不变，服务恢复，安装前窗口状态已保存。
3. 正确制品安装并重启；版本和辅助程序匹配；设置、设备身份、导入快照保留，统计仍为 24K。

证据：desktop-update-e2e-win32.json、desktop-update-e2e-linux.json。最后回归的升级发起端使用当前宿主源码，两个报告的 baseline_source_sha256 与当前 main.rs、native.rs 内容哈希相同；目标是冻结的已签名 1.0.2 测试制品。测试密钥、运行文件和测试产品未加入 Git，测试权限未授予生产 WebView。正式开发包没有配置更新公钥/端点，界面明确显示未配置渠道。临时更新签名不等于应用签名或公证。

Windows 安装器启动后的异步失败、安装中途磁盘故障、macOS 更新及正式更新源仍需验证。恢复策略是保留业务目录、重新打开旧应用或重新安装完整版本，不宣称所有回退路径均已验证。

## 性能与证据

合成负载：1,000 会话、100,000 条用量事件、12M tokens。Windows 冷启动约 5.70 秒、摘要中位数 1.6 ms；Linux 本机文件系统约 2.01 秒、1.4 ms。WSL 共享 C 盘的一次并行负载测试启动约 45.9 秒；文件系统和负载条件不同，不作直接跨系统比较。这不是大快照、长期运行或空闲资源的完整性能验收。

未提交的 output/verification/ 保存：

- windows-install.json、linux-package.json：安装、移除和数据保留。
- desktop-performance-win32-x64.json、desktop-performance-linux-x64.json：合成负载。
- appimage-x11.json、appimage-wayland.json：WSLg 启动模式。
- desktop-concurrency-win32.json、desktop-concurrency-linux.json：并发冷启动与配置隔离。
- final-native-files.json、linux-appimage-native-export-import.png：最后原生文件操作回归。
- windows-export-tooltip.png：先前轮次导出后已打开的提示框。
- windows-narrow-keyboard-tooltip.png：约 800 像素宽窗口中的已打开键盘提示框。
- linux-appimage-wayland-details.png、linux-webkit-keyboard-tooltip.png：实际 AppImage/WebKit 详情与已打开提示框。
- final-windows-dashboard.png、final-linux-dashboard.png：最终安装包对应程序的真实看板，两端显示相同的 24K 虚构用量。

截图属于对应图形轮次，最终二进制身份以最后复核 JSON 为准。

2026-10-24 最后复核的隔离实例（虚构日志与导入快照，合计 24K）：Windows http://127.0.0.1:6704/?period=all，Linux/WSLg AppImage http://127.0.0.1:46365/?period=all。两个健康接口均与当前 worktree 的辅助程序 `--build-info` 一致；Windows build ID 为 `9efe609013130a0e176c31dd3ec18501d8543c98185f742c6aa1157f82d286c5`，Linux 为 `022fe58611528364f028443895bfb5321164add1e4575263ddb48a57424442b4`。地址仅在对应验收实例仍运行时有效。

最终安装包 SHA256：Windows NSIS `b36898f97c7ff9541b425c817cd99f76331b2d525812e5cc9dfadb5cc4123e94`；Linux AppImage `5040821d0d32def2c0ccc8ea101bf3a90567408acb702b3d9224bab935daa1e2`；Linux `.deb` `b0a5e74ba478c4643fee878cca5a61ba133059ad4566429d3e6ed6dcffa48b02`。实际文件与各平台 artifacts.json 已逐字节校验。

## 未验证项

- macOS 两架构构建、WKWebView、Applications/Dock、菜单、原生文件、安装移除及更新；用户已确认保留。
- Windows 10、无 WebView2 的全新机器和离线运行库安装。
- Ubuntu 22.04/Debian 12 基线；独立 X11/Wayland、FUSE 和无托盘桌面的完整交互。
- 各端全部提示框位置、层级、鼠标、键盘和模态场景；单元测试及部分截图不能替代完整矩阵。
- 真实登录启动、多显示器切换、高 DPI、睡眠唤醒、长期运行、大快照与完整性能测量。
- 正式 Windows 证书、macOS Developer ID/公证及正式更新签名/发布资源。

Agensi 未接入，也没有成为客户端、数据或更新依赖。开发包独立可用；完整跨平台验收和正式发行就绪仍须分别完成。
