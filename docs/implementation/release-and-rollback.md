# 原生 Rust：安装、发布与回退

Rust 是唯一运行方式。生产 Python 后端、启动器、导出、桌面快捷方式和安装脚本已删除。
首页 `/` 直接提供重构后的 Svelte 页面；经典页面及其专属接口已删除。
Rust 程序只嵌入新版资源，直接运行不需要 Python、Node 或 Rust 工具链。

## 构建与离线安装

开发机需要 Rust stable 和 Node.js 24+（安装 skill 的运行检查只需要 Node.js 18+）：

```bash
node tools/package_release.mjs
node bin/install.js --runtime-manifest dist/release/manifest.json --offline
```

制品在 `dist/release/`：平台可执行文件、`manifest.json`、`SHA256SUMS`。
安装器验证 SHA256、平台、版本和可执行文件报告的 build ID 后才替换已有程序。
失败安装保留原程序和 manifest。Node 是安装工具，不参与正常启动。

npm 默认安装到 `~/.agents/skills/codex-usage-dashboard`；显式设置 `CODEX_HOME`
时使用 `$CODEX_HOME/skills/codex-usage-dashboard`。skills CLI 安装路径以其输出为准。
技能文件缺少原生程序时，可在技能目录运行：

```bash
node scripts/install_runtime.mjs --ensure
# 或离线安装
node scripts/install_runtime.mjs --manifest /path/to/manifest.json --offline
```

在线安装读取 `native.json` 指定的 版本对应的 GitHub Release manifest 和相邻平台文件。
每次调用 skill 都先执行 `--ensure`，校验版本、SHA256、build ID 与 `ui: svelte`；
skills CLI 更新遗留的旧 runtime 会自动替换，匹配版本无需联网。
新安装与升级均清理旧 Python 脚本。经典界面的制品会被拒绝。
不支持的平台、下载失败或缺少版本均报错，不提供替代后端。
发布前可以通过离线制品完成安装验收；新用户的默认在线安装需等对应 Release 正式发布。

同一 Release 可执行文件也支持直接下载运行，无需 skill、Node、Python 或 Rust。
Windows 下载 `.exe`；macOS/Linux 下载对应架构的文件并授予执行权限。
运行后仍通过浏览器使用本地面板。手动更新前先用旧程序的 `--stop` 退出。
发布目标为 `linux-x64`、`windows-x64`、`macos-x64`、`macos-arm64`，两种 Mac 架构分别原生验收。

## 启动、退出、导出

安装后的 `runtime/codex-usage-dashboard`（Windows 加 `.exe`）是唯一入口：

```bash
runtime/codex-usage-dashboard                       # 后台启动/复用并打开浏览器
runtime/codex-usage-dashboard --no-browser          # 后台启动，不打开浏览器
runtime/codex-usage-dashboard --stop                # 认证后正常退出
runtime/codex-usage-dashboard --serve               # 前台运行
runtime/codex-usage-dashboard --shortcut            # 创建桌面/应用菜单入口
runtime/codex-usage-dashboard --export-snapshot snapshot.json
runtime/codex-usage-dashboard --once --json
```

可重复传 `--codex-home PATH`，用 `--no-auto-windows` 禁用 WSL 的 Windows 来源发现。
启动器比较 build ID 和来源范围；变化时用 PID/build/instance 与私有令牌验证旧服务，
等进程退出、索引锁和 8765 释放后启动新版。并发启动由文件锁串行化。
未知或非托管监听者不终止、不强杀、不换端口。旧 Python 服务须手动正常关闭后迁移。
`--serve` 用于显式服务器绑定；默认后台启动仅绑定 `127.0.0.1:8765`。

Windows 在连接探测未发现监听者、但绑定仍返回 WSAEADDRINUSE 时，会在固定端口上
最多等待 5 分钟，供先前 Windows/WSL TCP 绑定回收；发现可连接的监听者即报错。
等待绑定时尚未打开索引，启动器的总超时为 6 分钟，等待期间输出日志路径。
后台进程不会继承调用方的标准输入输出管道，避免 Node/skill 在收到启动地址后仍等待 EOF。
本机观察到的 WSL 镜像绑定行为与[微软的 localhost 实现说明](https://github.com/microsoft/WSL/blob/master/doc/docs/technical-documentation/localhost.md)一致。

一次性导出使用临时索引，可与常驻服务并存。`--once` 包含已导入远端数据；
导出只含本机数据。原日志、设备身份、用户配置和手工导入快照保持原位置。

## 升级与原生回退

安装器保留版本化文件和前一个 manifest；回退也只运行经过校验的 Rust 制品：

```bash
node scripts/install_runtime.mjs --rollback
runtime/codex-usage-dashboard
```

npm 更新成功后删除旧安装目录中已知的 Python 脚本，保留外部配置与快照。
升级稳定入口前先正常关闭托管服务，以兼容 Windows 对运行中可执行文件的锁定。
不删除 Codex 日志或 `remotes/`。`index-v1/` 是独立可重建缓存。

## 发布与验收

`.github/workflows/native.yml` 在 Ubuntu 22.04、Windows 2022、macOS 15 Intel 和 Apple Silicon 上构建并测试。
PR、main 推送和手动运行都会合并四个平台制品，生成 `release-candidate` CI artifact，不创建 Release。
开发分支的普通推送不触发工作流；通过 PR 运行验收。
只有 `v*` 标签的 push 事件才允许发布，且必须通过全部验收、标签版本检查和 main 祖先检查。
发布使用 `docs/releases/vVERSION.md` 的说明。推送版本标签后，立即创建同名草稿 Release，并上传已验收、使用长期密钥签名的桌面暂存文件（含 `latest.json` 和 `SHA256SUMS`）。标签工作流等待跨平台验收后，从草稿下载桌面文件、合并 CLI 制品和校验文件，再发布草稿。草稿或桌面文件缺失时工作流会失败，不会发布不完整版本；补齐后可重跑失败任务。
合并工具拒绝缺失/重复平台、校验失败、版本不匹配和非空输出目录。未执行的 CI 不算验收通过。
完整操作边界见[发布准备清单](../releases/prepare-1.0.1.md)。

```bash
node tools/verify_native.mjs --replace-worktree
```

本地 Linux/WSL 验收将 PATH 设置为空，覆盖首次离线安装、缺失制品、根路径新版及静态资源、旧书签跳转、
重复/并发启动、导出、来源切换、校验失败保留旧安装、升级清理旧脚本、skills CLI 更新后的 runtime 替换、原生回退、
SSE 下退出和陌生端口不终止。结果见 `native-only-linux-x64.json`。
Windows `1.0.1` MSVC 制品已在原生 Windows 进程中完成上述全部生命周期验收，
并增加真实 socket 回归：绑定存在但尚无监听者时先等待，释放后仍在 8765 启动。
标准输出被捕获的 Node/skill 调用也会正常返回，结果见 `native-only-windows-x64.json`（passed=true）。
Windows Chromium 已验证根路径概览、设置、导航历史、快照管理和 320px/390px 布局。
macOS 完整验收仍需对应 CI；本地验收不代表已发布 GitHub Release。
Python 仅用于可选的历史差分测试，不是构建、打包、安装或运行的前提。

## Release 说明格式

后续 Release 沿用 [1.1.0 的简洁双语格式](../releases/v1.1.0.md)，每个版本只维护一份中英对照说明，不按语言拆分 Release。

- 用一两句话介绍版本，中英文含义一致。
- 下载表共用，放在正文前部；使用清晰的系统、架构和安装包名称。
- 功能或版本变化只列关键内容，中英文逐项对照。
- 安装与更新说明简短、符合该版本实际行为；需要用户处理的问题提供帮助链接。
- 命令行与 Skill 放在折叠区，校验、反馈和必要的回退说明放在末尾。
- 系统构建号、运行库版本、测试矩阵、签名过程和历史验收记录留在技术文档。Release 只保留影响用户下载、安装或使用的必要说明。

## 一个版本一个 Release

发布按一个版本一个 Release 整理：桌面安装包、命令行程序、更新清单和校验文件都在 `vVERSION` 下。下载名使用 `macos`，不附构建码；完整构建身份及 SHA256 保留在 `manifest.json` 和 `SHA256SUMS` 中。

CLI 仍使用原来的内部平台键（例如 `darwin-arm64`），安装器读取 manifest 中的新文件名，无需修改平台识别或牺牲完整性校验。桌面包与 CLI 的 Windows 文件分别使用 `-setup.exe` 和 `-cli-windows-x64.exe`，避免混淆。

经原生验收并完成长期密钥签名后，把桌面产物放到同一个 `vVERSION` 的暂存目录；不要再创建 `desktop-vVERSION` 或 `desktop-updates` Release。使用同一套校验文件合并工具，防止桌面清单覆盖 CLI 校验项：

```bash
node tools/merge_release.mjs downloaded-native dist/native vVERSION
node tools/desktop-update.mjs downloaded-desktop VERSION https://github.com/luyh7/codex-usage-dashboard/releases/download/vVERSION/ docs/releases/vVERSION.md dist/desktop --platform windows-x86_64 --platform darwin-aarch64
# 将已验收的 Mac DMG 同样按 macos-arm64.dmg 命名后加入 dist/desktop 及其 SHA256SUMS。
node tools/assemble-release.mjs dist/native dist/desktop dist/publish vVERSION
```

所有版本只使用一个 `manifest.json`、一个 `latest.json` 和一个覆盖全部资产的 `SHA256SUMS`。普通用户从 Release 说明顶部的安装包表下载；CLI 入口收在折叠说明中。签名、归档和机器清单仍作为同一 Release 的辅助资产保留。

用户选择移除独立更新频道，不保留兼容入口。已发布 1.0.1/1.1.0 客户端内置的旧地址将停止工作，需要手动下载完整安装包升级；保留配置和导入数据。后续构建使用统一入口 `https://github.com/luyh7/codex-usage-dashboard/releases/latest/download/latest.json`，并继续验证长期公钥更新签名及发行版本。此轮整理复用已验收的程序字节，未重新构建或扩大稳定支持范围。
