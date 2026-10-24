# CI

GitHub Actions 的统一入口为 [Client CI](../../.github/workflows/native.yml)。PR、main 推送和版本标签走同一入口；Actions 中手动运行 Client CI 会执行完整验证。桌面工作流通过 `workflow_call` 调用，不再为同一 PR 单独启动第二套入口。

## 变更判断与取消

`Plan validation` 使用 [ci-changes.mjs](../../tools/ci-changes.mjs) 判断本次推送的完整 before → after 差异，而不是仅看最后一个提交或整个 PR 的历史差异。

- 仅修改 `docs/`、Markdown、reStructuredText、根目录 LICENSE/NOTICE 时，不启动构建矩阵。仍保留一个成功的轻量判断 job，无需写 `[skip ci]`。
- 代码、依赖、工作流、测试夹具及其他未识别的非文档文件发生变化时，执行完整验证。多提交推送中即使最后一个提交只有文档，也会覆盖前面的代码变化。
- PR 新建、重新打开，以及无法读取 force-push 前历史时，回退到完整 PR 差异；仍无法判断时执行完整验证。重命名同时检查旧、新路径，不能通过将源码移入文档目录跳过验证。
- 手动运行和版本标签始终执行完整验证。

并发控制放在判断之后的可复用验证 job，按 workflow 和 PR 编号分组。新的代码推送会取消该 PR 的旧验证；文档推送跳过这个 job，不会取消此前尚未完成的代码验证。每次代码运行仍包含两个完整矩阵，避免新推送取消旧运行后遗漏旧运行尚未验证的共享代码。

main 和版本标签不启用自动取消。标签的发布和发布后下载检查保留原有权限与条件，必须等待验证成功。

## 检查分工

| 阶段 | 内容 |
| --- | --- |
| Shared checks and web build，一次 | 版本及脚本检查、发行组装/更新清单/CI 路由测试、两个 Rust workspace 的格式检查、Svelte 检查和前端测试、构建网页 |
| Native，四目标 | workspace clippy/测试、原生程序及 examples、协议/来源隔离/HTTP 安全、Python 参考结果、安装与旧入口兼容、原生包校验；Windows 额外检查运行库依赖 |
| Desktop，四目标 | 当前网页与匹配辅助程序的打包、桌面 all-feature clippy/单元测试、真实宿主启动、并发与窗口状态；Linux 额外检查 GLib 启动项；Mac 两架构保留包资源、ad-hoc 签名和六项签名更新/恢复测试 |
| Assemble release candidate | 四份 Native 制品均通过后组装，保留完整平台及摘要校验 |

四目标保持 Windows x64、macOS arm64、macOS x64、Linux x64；Linux 打包基线仍为 Ubuntu 22.04。共享检查通过后两个矩阵并行，矩阵内不因一个平台失败而取消其他平台的诊断。

前端校验和构建由原先八个平台 job 各做一次，收敛为一次。`web-dist` 制品传给两个矩阵；Desktop 使用 `bundle --skip-web`，不会再次安装网页依赖或构建网页。Rust workspace 测试和通用协议/安全检查保留在每个平台的 Native job，Desktop 不再重复执行。

## 缓存与制品

- `actions/setup-node` 缓存 npm 下载，分别使用 web 和 desktop 的 lockfile；不缓存 `node_modules`。
- `Swatinem/rust-cache` 缓存 Cargo 下载和依赖编译结果。Native 按平台区分，Desktop 按 Rust target 区分，并覆盖根目录和 desktop 两个 target 目录；动作还将工具链、manifest/lockfile 和相关编译环境纳入键值。
- 不缓存 workspace 自身的编译产物。辅助程序、宿主及安装包仍由当前源码构建并检查身份，不用缓存命中代替测试。
- `web-dist` 是同一次运行中共享的当前构建制品，保留 3 天。正式桌面制品在构建隔离更新测试产品之前上传，测试控制接口和临时签名密钥不进入生产交付包。

第一轮会冷构建并写入缓存。未观察到后续命中前，不承诺具体加速比例。

## 本地检查

```bash
node --test tools/ci-changes.test.mjs tools/release.test.mjs tools/desktop-update.test.mjs
actionlint .github/workflows/native.yml .github/workflows/validate.yml .github/workflows/desktop.yml
```

路由测试使用真实的临时 Git 仓库，覆盖文档推送到代码 PR、多提交推送、force-push、缺失历史、重命名、main/标签/手动运行和实际 `GITHUB_OUTPUT` 输出。
