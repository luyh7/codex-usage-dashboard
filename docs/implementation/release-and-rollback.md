# Rust / Svelte 候选版：安装、升级与回退

本次实现保留 skills CLI、npm、Python 启动脚本、导出脚本和桌面快捷方式入口。
已安装并校验本机原生制品时，`open_dashboard.py` 自动运行 Rust 后端和原版界面；
首页 `/` 和 `/index.html` 已恢复原版布局、配色及交互；Svelte 候选页面保留在 `/svelte`。
没有制品时继续使用随包提供的 Python 版。启动时不联网下载。
Windows/macOS 的完整发布门禁尚未全部通过，所以没有移除 Python 兼容入口，
也没有发布 GitHub Release 或 npm 版本。

## 本地构建与制品

开发机本次验证版本为 Node 24、Rust 1.96、Python 3.14；原生程序运行时不需要 Node/Rust/Python。
兼容启动器和安装脚本目前仍需要 Python。

```bash
python3 tools/package_release.py
```

输出在 `dist/release/`：本机架构的可执行文件、`manifest.json`、`SHA256SUMS`。
Svelte 静态资源已经嵌入可执行文件，不需要单独部署 web/dist。文件名包含版本、
平台和 build ID；manifest 同时绑定 SHA256、build ID 和版本。
应在目标平台构建、测试后分发相应 manifest 和相邻可执行文件。
`.github/workflows/native.yml` 为 Linux、Windows、macOS 准备了原生测试和候选制品任务，
不会自动发布。CI 尚未在远端运行；不能把工作流文件当作平台通过证据。

## 在现有安装中启用候选版

把 manifest 和对应二进制放在同一目录，然后运行安装目录内的脚本：

```bash
python3 ~/.codex/skills/codex-usage-dashboard/scripts/install_runtime.py /path/to/manifest.json --offline
python3 ~/.codex/skills/codex-usage-dashboard/scripts/open_dashboard.py --backend rust
```

Windows 使用 `python` 和实际安装路径。npm 安装入口也接受：

```bash
node bin/install.js --runtime-manifest /path/to/manifest.json --offline
```

只有显式运行安装器、且 manifest 指定 HTTPS URL 时才允许下载；下载失败、
离线文件缺失、SHA256 不匹配或 build ID 不匹配均保留原安装并返回错误。
安装器不会自行选择未经校验的 PATH 中程序。manifest 应来自可信的制品分发者。

端口固定为 8765。同一构建、同一显式日志范围重复启动会复用已有服务。
更换构建时，启动器用私有进程记录、health 的 PID/build/instance 标识和随机退出令牌
正常关闭由它管理的 Rust 服务；SSE 连接也会关闭。旧 Python 进程在 Linux/macOS
上必须与实际端口监听者及安装脚本的完整路径一致，才发送 SIGINT。
无法确认的服务不会被终止，也不会改用另一个端口。
直接启动、未交给启动器管理的 Rust 进程需要先自行 Ctrl+C。
Windows 的旧 Python 进程替换不在已通过范围内；先正常关闭旧窗口再升级。

可用 `--codex-home PATH`（可重复）和 `--no-auto-windows` 明确数据范围。
`--no-browser` 仅启动服务。原生程序保留 `--once --json`、`--export-snapshot [PATH]`、
`--host`、`--port 8765`、`--open`，并新增 `--build-info`、`--timezone`。
`--parse-workers` 继续接受；Rust 索引写入使用串行后台写者。
一次性 CLI 使用私有临时索引，能与常驻服务共存；`--once` 包含远端数据，
导出仍仅含本机数据，并支持相对输出路径。

## 数据保留与回退

导出继续使用 v1 schema；旧快照，包括仅存摘要或部分字段的快照，仍可导入。
原配置位置、设备身份和手工导入快照保留。Rust 在宿主配置目录的 `index-v1/`
维护独立、可重建索引，版本不匹配或索引损坏时重建；Python 缓存不被迁移或删除。
Unix 下索引目录权限为 0700、数据库为 0600。原日志始终只读。

原生升级保留旧版本文件和前一个 manifest：

```bash
python3 ~/.codex/skills/codex-usage-dashboard/scripts/install_runtime.py --rollback
python3 ~/.codex/skills/codex-usage-dashboard/scripts/open_dashboard.py --backend rust
```

也可以直接切回随包 Python 版：

```bash
python3 ~/.codex/skills/codex-usage-dashboard/scripts/open_dashboard.py --backend python
# 或长期选择：COUSASH_BACKEND=python
```

不要为了回退删除 `remotes/`、设备身份文件或 Codex 日志。仅在服务关闭后，
如需手工重建，可移走 `index-v1/`；这不是导入快照存储目录。
npm `--force` 更新技能文件时会保留 `runtime/` 以及其回退制品，配置和快照位于技能目录外。

## 可重放验收

`tools/verify_native.py --replace-worktree` 在临时 CODEX_HOME/config 中使用真实 8765：
首次安装、缺少制品、离线安装、损坏校验、失败升级、强制更新保留数据、manifest 回退、
陌生端口占用、CLI 导出、冻结旧 Python 安装升级、同构建复用、带 SSE 的正常退出，
以及 Python/Rust 切换。结束后恢复当前 worktree 服务。
`--replace-worktree` 只供本地 Linux 验证器识别当前 worktree 进程；CI 默认要求端口空闲。
结果见 `benchmarks/implementation/results/native-linux-x86_64.json`。

原生 Windows GNU 核心的 21 项测试已实际运行，见 `windows-core.json`。
它不覆盖原生浏览器、MSVC 制品、完整安装/升级或 macOS；这些仍为未通过的发布门禁。
Tauri 客户端属于方案阶段 7，本次没有实现。
