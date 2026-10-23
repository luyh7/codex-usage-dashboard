# 1.0.1 发布准备

用户入口有两种：安装 Codex skill，或直接下载 Release 可执行文件。skill 自动下载、校验并运行同一份文件。此阶段只准备并验收，尚不合并 main、不创建版本标签、不发布 GitHub Release 或 npm 包。

## 本地检查

每一步成功后再执行下一步。Windows 重建前，先确认现有 8765 进程属于当前工作区，并用已有程序的 `--stop` 正常退出，避免运行中的 `.exe` 被锁定：

历史差分测试固定使用 Python **3.14.5**，与冻结旧版语义时的解释器一致。后续 Python 补丁修复了小于一秒的时区偏移解析行为，不能作为同一个历史基准直接替换。Python 只用于开发验收，不随应用分发，也不是用户运行依赖。

```text
npm ci --prefix web
npm run check --prefix web
npm test --prefix web
npm run build --prefix web
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --examples --bin codex-usage-dashboard --locked
python tools/verify_core.py
npm run validate
node tools/check_release.mjs v1.0.1
node --test tools/release.test.mjs
node tools/package_release.mjs --skip-build
node tools/verify_native.mjs --replace-worktree
```

先确认 8765 的进程身份。验收脚本仅通过原生认证退出已有托管程序，拒绝不相关的监听者；结束后重新启动当前工作区。交付前比较 `/api/health` 与本地程序 `--build-info` 的 build ID，确认版本为 1.0.1、`ui: svelte`、`index.phase: ready`。

原生验收使用隔离的配置和虚构日志，覆盖直接下载运行、skill 安装文件与直接下载逐字节相同、升级、损坏制品保护、快照、回退、进程复用和退出。通过模拟 Release 传输验证安装器下载路径；它不能证明尚未发布的公共下载地址可用。

浏览器发布抽查使用 `tools/verify_brand_navigation.mjs` 和 `tools/verify_settings_page.mjs`，通过 `PLAYWRIGHT_MODULE` 指向 Playwright 模块、`PLAYWRIGHT_EXECUTABLE_PATH` 指向 Chromium。覆盖概览/设置/详情导航、时区与语言持久化、快照管理和 320px/390px 布局。早期 `verify_unified_overview.mjs` 仍断言已经被品牌图替换的文字标题，不作为当前版本验收入口。

历史差分对照保留 token、价格和费用的精确比较；Windows 路径和由路径生成的 UID 统一到测试占位符。冻结 Linux 样本的文件 `updated_at` 允许 1 微秒的平台精度差异。新版价格有效区间、最近用量和新增汇总维度由 Rust 专项测试验证，不要求与旧 Python 的展示字段一致。

## GitHub 候选制品

1. 推送开发分支并创建面向 main 的草稿 PR。
2. 等待 `linux-x64`、`windows-x64`、`darwin-x64`、`darwin-arm64` 全部通过。不要以旧的本地报告替代当前提交的 CI。
3. 确认 `Assemble release candidate` 成功，并取得 `release-candidate` artifact。内容应为四个可执行文件、一个合并的 `manifest.json`、一个 `SHA256SUMS`。
4. 对照发布说明确认安装方式、平台范围、旧 Python 迁移和未签名状态。CI artifact 下载为 ZIP；正式 Release 将上传其中的独立文件。

版本检查覆盖根 package、前端 package/lock、Cargo workspace/lock、skill native.json、发布说明和版本标签。发布矩阵采用明确的系统版本与架构，并在打包时验证运行器实际架构。

## 正式发布时再执行

以下步骤需要用户明确要求正式发布，本轮不执行：

1. 审核并合并 PR 到 main，确认合并提交的 CI 全部通过。保留 Git 历史中的历史测试基线；不要移除 `tools/reference_dashboard.py` 所引用的提交。
2. 在确认的 main 提交创建并推送 `v1.0.1` 标签。标签必须指向 main 历史中的提交，不能直接给未合并开发分支打发布标签。
3. 标签 CI 重新构建、测试、合并制品后自动创建 Release，使用 `v1.0.1.md` 作为发布说明。不提前手动创建同名 Release，不覆盖既有版本文件。
4. 从实际发布地址，在干净的 skill 目录验证首次下载及升级；直接下载各平台文件验证运行。比较 SHA256、版本和 build ID。
5. 检查 README 两条安装入口。main 合并到 Release 完成之间，新 skill 安装可能因制品尚未就绪而失败，发布操作应连续完成。

GitHub Release 发布前无法完成真实公共下载的最终验收。这是正式发布后的必要检查，不是本地模拟或 PR CI 的替代项。
