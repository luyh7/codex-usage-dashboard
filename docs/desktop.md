# 桌面客户端

客户端使用 Tauri 2，共享 Rust 统计核心和 Svelte 看板。安装包包含同版本统计程序，不需要安装 Node、Python、Rust、Skill，也不需要登录平台。Agensi 没有成为客户端依赖。

## 平台与交付范围

| 平台 | 架构 | 安装形式 | 目标基线 |
| --- | --- | --- | --- |
| Windows | x64 | 每用户 NSIS `.exe`、开始菜单 | Windows 10/11，WebView2 |
| macOS | arm64、x64，分别打包 | `.app`、`.dmg` | 配置下限 14.5，尚待最低版本实测 |
| Linux | x64/glibc | AppImage、`.deb` | 目标 Ubuntu 22.04 / Debian 12；必须在对应基线上打包验证 |

**目标范围不是已验证承诺。** 2026-10-24 用户将本轮目标明确为“稳定版准备，仅承诺已验收的环境”。首发候选范围如下；当前制品仍是开发包，正式签名及渠道验收通过前不标记为稳定发行。最新证据见 [审查与发行准备记录](desktop-review-2026-10-24.md)，更早证据见 [验证记录](desktop-verification.md)。

| 首发候选环境 | 本轮证据与范围 |
| --- | --- |
| Windows 11 x64，系统 build 22631，已有 WebView2 155.0.4283.62 | 当前 CI 包宿主的界面、原生来源/文件操作、服务隔离，以及同源码隔离产品的严格更新/恢复检查通过；不承诺 Windows 10 或全新/离线 WebView2 部署 |
| macOS 26.6.2（25G83），Apple Silicon arm64 | 当前生产配置包的原生界面、大快照、来源与服务恢复，以及同源码隔离产品的严格更新/恢复检查通过；不扩展为最低 14.5 或 Intel 承诺 |

Linux AppImage 在 Ubuntu 26.04 / WSLg 的当前包烟测、并发及同源码更新/恢复通过，暂保留实验性质；Intel Mac 和其余未验收系统也不进入首发稳定更新清单。多屏、高 DPI 切换、睡眠唤醒、真实登录启动和长期运行仍未完整验收，不能用上述核心流程检查覆盖。在更新系统上生成 AppImage 不代表它能在旧系统上运行。Windows ARM、Linux ARM/musl 和 macOS Universal 不在首批范围内。

Windows 安装器在没有 WebView2 时联网安装运行库；离线环境须预先安装 WebView2。Linux `.deb` 声明 WebKitGTK 4.1 和托盘依赖，AppImage 的运行仍受宿主内核、glibc 和图形环境限制。运行库与编译工具是不同的要求。

## 使用

Windows 从开始菜单启动；macOS 将 `.app` 放入 Applications 后打开；Linux 为 AppImage 添加执行权限后打开，或通过系统包管理器安装 `.deb`。升级 `.deb` 前退出客户端，完成后重新打开，以加载整套新版本。

首次打开自动发现本机 Codex 目录。设置可以查看当前来源，选择其他文件夹，或恢复自动发现。缺少目录、读取失败和空目录会显示状态。更换来源后客户端重启自己拥有的统计服务；使用独立 CLI 服务时会要求先退出该服务，不会擅自替换它。

客户端先检查服务归属再保存来源，拒绝操作时保留现有来源配置；停止服务失败时尝试恢复原来源。CLI 的摘要和导出也检查当前实际来源及程序身份，来源发生变化时明确报告冲突，避免静默使用旧索引。

首次没有可用本机日志且没有导入会话时，客户端直接打开日志来源设置，提供文件夹选择；空的 sessions 目录不会被当作已有用量。

设置中的语言和展示时区保存在应用配置里，端口变化、关闭重开和覆盖安装后仍可恢复。浏览器原有 localStorage 仅作为首次配置的迁移来源；不能自动读取另一个浏览器的数据。临时筛选与展开状态仍可由当前页面保存。

设备数据中可导入 `.json` / `.json.gz`，重命名导入设备，以及完整或脱敏导出。完整快照包含会话摘要和本地路径；脱敏模式保留用量和费用并移除或匿名化这些内容。桌面导出使用系统保存对话框、后台压缩和实际进度，写入成功前不会替换目标文件。取消导出不生成快照。

完整和脱敏导出共用上次成功保存的目录；目录已不存在时回到文档目录或用户主目录。取消或保存失败不会覆盖记录。日志文件夹选择记住上次选择，首次优先当前配置来源，再使用用户主目录。目录记录与业务设置、窗口状态分开保存，无法保存记录不影响已完成的导出或选择。

同一来源的完整与脱敏快照使用不同的设备标识，同时导入会分别计入总量。汇总同一份用量时，保留其中一种副本。

Windows 默认关闭窗口退出，可选择保留在托盘。macOS 关闭窗口保留应用，通过 Dock、菜单或托盘重新显示，Cmd+Q 明确退出。Linux 关闭窗口退出，保留显示、设置和退出菜单；不提供可能在无托盘桌面中无法恢复的隐藏设置。登录启动默认关闭，由用户开启；开发配置不注册开机启动。后台启动遇到无托盘或 Linux 会话时保持窗口可恢复。

窗口尺寸、位置和最大化状态按配置范围保存；显示器变化后检查位置是否仍可见。菜单提供最小化、最大化、全屏和关闭。Cmd/Ctrl+, 打开设置，Cmd/Ctrl+Q 退出；Windows/Linux 的全屏快捷键是 F11。

## 路径与进程

| 内容 | 位置 |
| --- | --- |
| Windows 业务配置 | `%APPDATA%/cousash` |
| macOS 业务配置 | `~/Library/Application Support/cousash` |
| Linux 业务配置 | `$XDG_CONFIG_HOME/cousash`，默认 `~/.config/cousash` |
| 长期设置 | 配置目录下 `preferences.json` |
| 窗口设置 | 配置目录下 `desktop-settings.json` |
| 文件对话框目录 | 配置目录下 `desktop-file-dialogs.json` |
| 运行记录、启动锁和日志 | Windows：配置目录下 `runtime/`；Linux：`$XDG_CACHE_HOME/cousash/scopes/<配置路径哈希>/runtime`（默认 `~/.cache`）；macOS：`~/Library/Caches/cousash/scopes/<配置路径哈希>/runtime` |
| 可重建的 WebView 缓存 | Windows：配置目录下 `cache/webview`；Linux：上述私有范围目录下 `webview/`；WKWebView 使用按配置范围派生的数据存储标识 |

`COUSASH_CONFIG_DIR` 显式覆盖配置范围；支持绝对路径、当前目录下的相对路径和 `~`。保留既有设备身份、索引和导入快照，不迁移或删除旧业务目录。卸载器处理安装文件和入口，默认保留业务数据。不要将运行记录、日志、缓存、真实快照或签名私钥提交到 Git。

桌面只从自身旁边定位随包统计程序，使用前核对其版本及构建身份，不搜索 PATH。服务实际绑定 `127.0.0.1:0` 后发布就绪记录，没有先探测再释放端口的竞争。普通浏览器启动首次仍请求 8765，已有同配置实例时复用其实际地址；`--serve --port` 保持原有前台用法。

CLI 和桌面共用 `usage-runtime`：有界启动锁、独占服务锁、原子记录、随机实例身份和健康核验。记录包含 schema、地址、PID、版本、build ID、协议、配置范围及来源。随机控制令牌只存在受当前用户保护的运行文件中。发现损坏记录时，仅在没有现存服务写锁的情况下隔离旧记录；未知监听者和身份不符的进程不会被停止。

关闭桌面只停止它拥有的服务，保留连接到的独立服务。桌面退出后，辅助程序通过 Windows 进程句柄或 Unix 父子关系检测拥有者消失，避免仅依赖可重用的 PID。运行中服务退出后，宿主尝试恢复；恢复失败显示日志位置与重试入口。单实例激活通道按配置隔离，不会聚焦其他工作树。

原有 loopback、Host、Origin、Fetch Metadata 和写 token 检查继续生效。WebView 导航受限于应用页面；外部 HTTP(S) 链接交由系统浏览器。原生命令只授予已核验服务的确切地址，并在文件和设置操作前再次核验服务身份。没有授予通用 shell 或任意文件读写能力。

## 本地命令

使用安装目录内的 `codex-usage-dashboard`，Windows 加 `.exe`：

```text
codex-usage-dashboard open --json
codex-usage-dashboard status --json
codex-usage-dashboard summary --period today --json
codex-usage-dashboard summary --period custom --start 2026-10-01 --end 2026-10-07 --json
codex-usage-dashboard export --output /absolute/path/snapshot.json.gz --redact --json
```

新命令始终输出一份 JSON，接受 `--json` 便于显式声明机器调用。成功结构为 `{"schema_version":1,"ok":true,"data":...}`；失败为 `{"schema_version":1,"ok":false,"error":{"code":...,"message":...}}`。成功退出 0，操作失败退出 1，clap 参数解析失败退出 2。错误代码包含 `desktop_not_installed`、`timeout` 和 `operation_failed`；诊断信息不含控制令牌。

`open` 打开或聚焦真正的客户端，没有安装宿主时明确失败。`status` 不启动窗口、服务或索引。`summary` 和 `export` 复用已有索引；没有服务时启动不显示窗口的本机服务。摘要日期边界采用应用保存的展示时区。完整导出是默认模式，`--redact` 脱敏；输出目录必须存在，失败保留已有目标文件。

启动锁最多等待 90 秒，服务就绪最多等待 90 秒，二进制身份检查最多 5 秒；健康请求最多 10 秒，索引等待 90 秒，导出传输最多 10 分钟。版本、协议、配置或来源冲突需退出所属实例或使用另一配置目录，不自动停止其他工作树。旧 `--build-info`、`--once --json`、`--export-snapshot`、`--launch`、`--stop`、`--shortcut` 和前台服务入口保留。历史 `launcher-process.json` 仅用于经核验的旧服务退出。

## 开发与打包

先安装当前平台的 [Tauri 编译依赖](https://v2.tauri.app/start/prerequisites/)，再在任务 worktree 内执行：

```bash
npm ci --prefix web
npm ci --prefix desktop
node tools/desktop.mjs build
```

启动开发实例前，为当前 worktree 设置独立的 `COUSASH_CONFIG_DIR`，再运行 `node tools/desktop.mjs run`。不要停止其他 worktree 的服务。`prepare` 只构建、定位和复制辅助程序；`bundle` 构建发布 profile 并生成当前平台安装包：

```bash
node tools/desktop.mjs bundle
node tools/desktop-smoke.mjs
node tools/desktop-concurrency.mjs
```

macOS 本地开发包需要完整 ad-hoc 应用签名时，使用 `APPLE_SIGNING_IDENTITY=- node tools/desktop.mjs bundle`，与开发 CI 的签名方式一致；随后运行 `codesign --verify --deep --strict` 检查 app。仅由链接器签名的 Mach-O 不等于完整应用包签名，也不代表 Developer ID 或公证。

制品位于 `desktop/target/<target>/release/bundle/`；同级 `artifacts.json` 记录版本、服务 build ID、大小及 SHA256。`desktop/Cargo.lock` 和 `desktop/package-lock.json` 冻结桌面依赖；桌面是独立 Cargo workspace，不给普通 CLI 发布增加 WebKitGTK 或其他桌面编译依赖。

Windows 与 WSL 共用源码时，请隔离两边 Cargo 产物目录和 Node 平台依赖。例如 Linux 构建使用 `COUSASH_NATIVE_TARGET_DIR=target/linux`、`COUSASH_DESKTOP_TARGET_DIR=target/linux`。`--skip-web` 复用已经构建的同一份 `web/dist`，不能用来跳过未完成的前端构建。`DESKTOP_TARGET` 必须是本计划四种原生架构之一；构建脚本需要在对应系统运行随包程序检查身份。

[桌面 CI](../.github/workflows/desktop.yml) 覆盖 Windows x64、macOS arm64/x64 和 Ubuntu x64。烟测启动真实宿主、核对随包服务和虚构日志、重复启动并退出；它验证进程与服务集成，不能替代窗口、文件对话框、Dock、托盘或安装升级的交互验收。

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo fmt --manifest-path desktop/Cargo.toml --check
cargo clippy --manifest-path desktop/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path desktop/Cargo.toml --release --locked
npm run check --prefix web
npm test --prefix web
node --test tools/runtime.test.mjs tools/security.test.mjs tools/desktop-update.test.mjs
```

`AUDIT_BINARY` 可显式指定测试的统计程序，避免误用旧产物。最终交接须用当前运行实例 `/api/health` 对照相应随包二进制 `--build-info`，记录实际 URL；不同系统的 build ID 不要求相同。

烟测还检查显式退出保存窗口状态和 Unix 运行目录权限。用 `SMOKE_SESSIONS=1000 SMOKE_EVENTS=100` 可生成较大的虚构数据集；Linux 建议将 `SMOKE_WORK_DIR` 指向本机文件系统中的独立测试目录。WSL 共享 Windows 目录的文件访问和索引写入时间不宜直接代表普通 Linux 用户目录的性能。

## 更新与发行

设置提供检查更新、版本说明、下载进度以及用户触发的安装和重启。Windows、macOS 和 AppImage 采用 Tauri 更新器；`.deb` 提示通过包管理器或重新安装更新，不用应用更新器替换包管理器文件。开发包没有配置更新公钥或端点时明确显示未配置，不伪报已经是最新版本。

更新先下载并验证签名，再协调退出自己拥有的服务，整体替换应用和辅助程序。同步安装错误会尝试恢复当前服务；Windows 安装器启动后由安装器负责退出和重启，若安装器失败可重新打开旧应用或重装完整包。升级不删除业务数据。连接独立服务时要求先退出它。离线和下载失败保持当前版本可用；签名校验不能用 SHA256 代替。正式更新端点要求 HTTPS，且配置要求签名绑定发行版本。

发行时将 `desktop/release.example.json` 复制到受保护的本地配置，填写更新**公钥**和端点，通过 `DESKTOP_RELEASE_CONFIG` 指向它。签名私钥只通过 `TAURI_SIGNING_PRIVATE_KEY` 和相应口令环境变量或 CI secret 提供；不要把私钥写入配置、仓库、日志或安装包。

应用签名配置也放在同一受保护的发布覆盖文件中。Windows 在 `bundle.windows` 设置证书存储中的 `certificateThumbprint`、`digestAlgorithm: "sha256"` 和证书提供方的 `timestampUrl`；证书及私钥先由维护者安装到构建机的证书存储，具体服务型签名可使用 Tauri 的 `signCommand`。构建后分别对安装器、宿主和辅助程序执行 `signtool verify /pa /all`，任一漏签或校验失败都不作为可信发行包。配置依据见 [Tauri Windows 签名](https://v2.tauri.app/distribute/sign/windows/)。

macOS 使用钥匙串中已有的 Developer ID Application 身份，通过 `bundle.macOS.signingIdentity` 或 `APPLE_SIGNING_IDENTITY` 指定。公证凭据使用 `APPLE_API_ISSUER`、`APPLE_API_KEY`（Key ID）和 `APPLE_API_KEY_PATH`（私钥文件路径），或 Apple ID 对应的三项凭据；不把值写入仓库。构建后检查整个 app、包内辅助程序以及公证票据：`codesign --verify --deep --strict <app>`、`spctl --assess --type execute --verbose <app>`、`xcrun stapler validate <app>`。具体证书导入和公证步骤见 [Tauri macOS 签名](https://v2.tauri.app/distribute/sign/macos/)。当前 PR 的 CI 固定使用 ad-hoc 开发签名，不消费正式证书或公证凭据；正式签名构建及凭据注入尚未实测，不能把这些配置说明算作验证通过。

签名构建完成后，将所选平台的 `desktop-<Rust target>` 制品下载到同一目录，生成更新源。省略 `--platform` 时仍要求四种架构；首发候选范围使用显式选择：

```text
node tools/desktop-update.mjs <downloaded> <version> <https-base-url> <notes-file> <empty-staged-directory> --platform windows-x86_64 --platform darwin-aarch64
```

平台参数可重复，可选值为 `windows-x86_64`、`darwin-aarch64`、`darwin-x86_64`、`linux-x86_64`。工具要求每个选中目标都有相同版本的 build manifest 和更新签名；未知、重复或缺失目标会失败，不会静默缩小范围。输入的版本、构建标识、文件数量和签名文件格式检查全部通过后才写入；此处的签名格式检查不等于密码学验证，最终包仍须通过客户端公钥校验。输出目录必须为空，并用排他锁拒绝并发写入，避免平台或版本混包。生成不冲突的制品名称、只含选中目标的 `latest.json` 与校验清单；不上传或发布。实验平台使用独立空目录及渠道，不并入稳定清单。异常中断后保留现场，确认没有写入进程后换用新的空目录重新组装。Windows 应用签名、macOS Developer ID 签名/公证和更新签名分别管理。开发 CI 的 ad-hoc 签名或本地测试更新密钥均不代表正式发行信任。

稳定版发布前仍须完成：

1. 提供正式 Windows 签名、macOS Developer ID/公证和生产更新密钥/HTTPS 端点；按上述命令验证实际最终包。当前测试密钥和 ad-hoc 签名不作替代。
2. 用最终签名包在候选环境做干净安装、首次打开、完整包升级/移除与保留数据检查，并在真实生产渠道验证版本绑定、下载、签名、安装/重启及失败恢复。更新测试产品的通过不能替代这一步。
3. 冻结版本、制品摘要、支持说明和回退完整包；复查稳定更新清单只列已验收目标。取得公开发布授权后再上传、打标签或发布。

`node tools/desktop-update-e2e.mjs build` / `run` 在 Windows、macOS 两架构和 Linux AppImage 上构建并测试隔离更新产品，版本为 1.0.1 → 1.0.2。它使用本地源、临时密钥和 `desktop-tests` feature，所有测试数据留在忽略的 `output/` 内。测试接口只存在该 feature 构建中，没有加入 WebView 权限或生产默认配置。测试后必须重新执行正常 `bundle`，交付不带该 feature 的制品。

回退时保留用户配置、设备身份和 `remotes/`，重新安装已知可用的完整应用版本。不要混用旧宿主与新辅助程序。较新设置 schema 会明确拒绝读取；未来若增加不可逆配置迁移，发行前必须实现并验证备份/迁移与回退，再提供相应版本。当前版本没有迁移或删除既有业务目录。

## Agensi

当前交付不接入 Agensi 账号、支付、MCP 或分发规则。已有 Skill 可继续使用原生 CLI；将来的平台适配仅调用本地公开命令。客户端安装、数据和升级由客户端管理，删除 Skill 不应删除客户端数据。
