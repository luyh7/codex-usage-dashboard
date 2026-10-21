# 原生 Rust：安装、发布与回退

Rust 是唯一运行方式。生产 Python 后端、启动器、导出、桌面快捷方式和安装脚本已删除。
首页保持原版界面，来自 `web/classic/index.html`；Svelte 页面保留在 `/svelte`。
两套静态页面都由 Rust 服务提供。运行不需要 Python、Node 或 Rust 工具链。

## 构建与离线安装

开发机需要 Rust 和 Node 18+：

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
node scripts/install_runtime.mjs
# 或离线安装
node scripts/install_runtime.mjs --manifest /path/to/manifest.json --offline
```

在线安装读取 `native.json` 指定版本的 GitHub Release manifest 和相邻平台文件。
不支持的平台、下载失败或缺少版本均报错，不提供替代后端。
当前本地工作没有推送或发布；新用户的默认在线安装需等对应制品正式发布。

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

`.github/workflows/native.yml` 在 Linux、Windows、macOS 构建和测试原生制品。
版本标签触发通过后的 Release 发布，并合并各平台 manifest；制品版本必须与标签一致。
普通分支和 PR 仅上传 CI artifacts。未执行的 CI 不算平台验收通过。

```bash
node tools/verify_native.mjs --replace-worktree
```

本地 Linux/WSL 验收将 PATH 设置为空，覆盖首次离线安装、缺失制品、原版页面、
重复/并发启动、导出、来源切换、校验失败保留旧安装、升级清理旧脚本、原生回退、
SSE 下退出和陌生端口不终止。结果见 `native-only-linux-x64.json`。
Windows GNU 制品已交叉编译并在真实 Windows 进程中完成 build-info 与离线安装。
本机 Windows 端 8765 返回 WSAEADDRINUSE；独立 Node 监听器也返回 EADDRINUSE，
因此服务启动验收受主机端口占用阻挡，没有终止无法确认身份的进程。记录见
`native-only-windows-x64.json`（status=blocked，非通过）。Windows/macOS 完整验收仍需对应 CI。
Python 仅用于可选的历史差分测试，不是构建、打包、安装或运行的前提。
