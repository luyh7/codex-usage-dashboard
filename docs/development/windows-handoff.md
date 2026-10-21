# Windows 开发交接

## 分支与范围

- 开发分支：`codex/rust-svelte-redesign`，包含 Rust 后端、原生启动/安装，以及 Svelte 的 Clear Mint 页面重构。
- 稳定基线：`c42f0e8`（`Update OpenAI model pricing`）。本次不向 `main` 合并，不创建发布标签，不发布 npm 包或 GitHub Release。
- 页面原稿和规范：[Clear Mint](../design/clear-mint/README.md)。当前只有一个主题，Svelte 不提供深色或跟随系统模式。
- 默认 `/` 保留经典页面；新版位于 `/svelte`。

## 获取与运行

在 Windows 原生环境准备 Git、Node.js 24 和 Rust stable。使用默认 MSVC 工具链时需安装 Visual Studio C++ Build Tools 及 Windows SDK。以下命令在 PowerShell 执行，每一步成功后再执行下一步。

新目录：

```powershell
git clone --branch codex/rust-svelte-redesign https://github.com/luyh7/codex-usage-dashboard.git
cd codex-usage-dashboard
```

已有仓库且工作区干净时：

```powershell
git fetch origin
git switch codex/rust-svelte-redesign
git pull --ff-only
```

从仓库根目录构建并启动：

```powershell
npm ci --prefix web
npm run build --prefix web
$env:COUSASH_REQUIRE_WEB = '1'
cargo build --release --locked --bin codex-usage-dashboard
.\target\release\codex-usage-dashboard.exe --no-browser --timezone Asia/Shanghai
Invoke-RestMethod http://127.0.0.1:8765/api/health
Start-Process 'http://127.0.0.1:8765/svelte?period=week&timezone=Asia%2FShanghai'
```

每次修改后重新构建并运行当前工作区，固定使用 **8765**。前端资源嵌入 Rust 程序，因此必须先完成前端构建，再构建 Rust。启动器会复用相同版本的受管理进程，或认证后替换旧版本。遇到其他程序占用端口时先确认进程身份。

用 `--build-info` 与 `/api/health` 对比 `build_id`，并确认 `index.phase` 为 `ready`。健康检查中的 `ui: legacy` 指默认根页面；新版仍可通过 `/svelte` 访问。不要将 Linux 构建的 `target/` 或 `web/node_modules/` 复制到 Windows。

## 验证与继续开发

```powershell
npm run check --prefix web
npm test --prefix web
cargo test --workspace --locked
```

浏览器重点检查概览、项目展开/主任务与子代理详情、日历草稿/应用/取消、窄屏浮层和减少动画偏好。原始设计稿使用虚构数据；真实页面的项目名称和数值由本机日志决定。

已有 Linux 原生生命周期和 Svelte 浏览器验收记录。此前 Windows 经 WSL 互操作验证时遇到 8765 端口冲突，因此 Windows 原生服务器生命周期、浏览器和 MSVC 构建仍需在新环境验证，不能视为已完成跨平台发布验收。详细历史见 [实施记录](../implementation/progress.md)。

`tools/verify_mint_ui.mjs` 使用隔离的虚构 API 数据；目前其浏览器路径针对 Linux，Windows 可先通过浏览器完成视觉校验，再适配自动化入口。本机 `output/` 截图和临时输出不上传；已采用的三张原稿与字体许可证随仓库保存。

后续提交继续推送 `codex/rust-svelte-redesign`。待 Windows 验证和页面验收完成后，再单独决定合并和发布。
