# 当前状态：Rust 为唯一运行入口

2026-10-23：`v1.0.1` 已通过 Windows MSVC 构建和完整原生生命周期回归。
修复 Windows/WSL 旧绑定回收期间的启动失败，以及后台进程继承输出管道导致 skill 启动命令不返回的问题。
Ctrl+C 与认证退出都会通知 SSE 流结束，避免长连接阻止服务退出。
Windows Chromium 的概览、设置、快照操作与窄屏检查已通过。新制品尚未对外发布。

2026-10-21 后续调整已删除生产 Python 后端、启动器、导出、快捷方式和安装脚本。
2026-10-22 已移除经典界面和专属列表/详情接口，根路径直接打开 Svelte 新版。
原生制品与 skill 版本为 `1.0.1`；skill 每次启动前校验 runtime，自动替换更新后残留的旧版。
原生程序负责后台启动、
同版本复用、配置变更替换、认证退出及桌面入口；Node 仅用于安装/构建。
首次安装缺少原生制品时返回错误，不再回退 Python。源码安装和离线制品安装均可用；
默认在线安装需要发布对应版本的 GitHub Release，本次未推送或发布。
开发差分测试从固定 Git 提交临时载入 Python 参考实现，不随产品分发。

以下保留原重构阶段的验收记录，其中 Python 回退和旧启动器验证是历史记录，
不再描述当前产品行为。当前启动验收见 `native-only-linux-x64.json`。

# Rust / Svelte 实施与验收记录

阶段 0–6 的实现与本机验收已落地；这是带 Python 回退的候选版本，**完整跨平台发布门禁尚未通过**。
阶段 7 的 Tauri 客户端不属于本次交付。原生 Windows GNU 核心测试已运行；
Windows 浏览器/安装、MSVC 制品和 macOS 仍未验收，不能用 WSL Chromium 结果替代。

## 实现范围

| 阶段 | 交付 |
| --- | --- |
| 0 合同 | 13 组语言无关 golden、逐字段比较器、原始基线保留、差分与真实 HTTP 重放。 |
| 1 Python 渐进改动 | 100 事件分页、完整图表独立查询、搜索索引、派生结果缓存和局部 DOM 更新；旧详情与导出保留。 |
| 2 Rust 核心 | 强类型用量/事件/计价、流式 JSONL、追加恢复、续写/父子归一化、runtime tier、历史价格、期间/时区/日历查询。 |
| 3 索引 | 单写者 SQLite WAL、事务 revision/offset、不可变快照、压缩缓存、关系分量复用、库存缓存、watcher 加轮询、索引损坏重建。 |
| 4 Axum | 后台发布、受限阻塞查询、旧 HTTP 合同、新分页接口、SSE、固定端口与可验证构建身份、原生 CLI。 |
| 5 Svelte | 宿主无关数据接口、请求取消与过期丢弃、项目/任务分页、代理树、四指标、日历、完整曲线、远端管理、键盘/窄屏/主题/密度和 URL 状态。 |
| 6 发布准备 | 校验制品安装、离线失败提示、启动器选择/替换、旧安装升级、manifest/Python 回退、目标平台 CI 和本机制品。尚未外部发布。 |

原 npm/skills CLI/桌面快捷方式均继续调用兼容启动器；安装了校验通过的原生制品时运行 Rust，
否则继续使用 Python。Python 导出脚本仍可使用。没有 Node/Rust 开发工具链也能运行已编译的原生程序。
具体步骤见 [安装与回退](release-and-rollback.md)，接口规则见 [兼容合同](compatibility.md)。

## 正确性证据

- 原始 HEAD `c42f0e8ae948d4cde392fafa9894681a1200b17d`、Python SHA256
  `20fae68140fa8186ed86728aaac16d8d6688b1218d555e93e1a49e4761319135` 与旧报告相符。
  用户已有的研究、方案及 `benchmarks/refactor/results/` 原始证据保留。
- Python 94 项测试通过（原 90 项加 4 项分页检查）；13 组 golden 重放通过。
- Rust 39,543 个精确差分案例通过，含 35,235 个价格边界、3,031 个时间语法案例和费用 fuzz。
  身份、所有公开字段、整数、字段存在性和六位小数金额逐项比较。
- 26 项旧测试通过 Rust 原始解析器桥接；此桥接的关系归一化/索引部分仍由 Python 执行，
  独立 Rust 关系归一化和持久索引另由 golden/full-field 路径验证。
- 4 项旧远端测试加 1 组旧价格分段 fixture 比较确认流程、合并后的存档和转换后的所有公开字段，另比较旧稀疏快照的日期范围结果。
- Rust 21 项单元测试在 Linux 和实际 Windows 原生进程中通过，覆盖计数溢出、DST、Git worktree、
  部分末行、追加/截断/等长修改/归档移动、损坏缓存、写者锁、重计价、依赖分量复用，以及三个写入阶段的真实子进程崩溃恢复。
- 真实 8765 HTTP 对照覆盖全量列表/详情/日期、204/stale、10,003 个事件全页无遗漏、完整曲线、
  v1 导出/设备身份、远端 CRUD、旧 revision 的来源信息，以及四客户端与追加/导入并发。
  另验证 100 个无关插件缓存目录不会被递归监听；仅日志目录递归监听，辅助文件由根目录非递归监听覆盖。
- Chromium 真实操作覆盖键盘详情、分页、组合筛选、浏览器刷新、日期排除、远端管理、窄屏、
  SSE 断线重连、事件/任务页跨 revision 保持、跨页选择、代理展开和失败重试。
- TypeScript/Svelte 检查无错误/警告，HTTP adapter 的取消/晚响应/204/revision 测试通过，strict clippy 通过。
- 临时安装目录重放覆盖冻结旧版 Python 安装升级、离线安装/校验失败、配置/快照保留、回退、
  陌生端口不终止、带 SSE 的正常退出和端口释放。

原始结果位于 `benchmarks/implementation/results/`：`core-correctness.json`、
`parser-bridge.json`、`remote-bridge.json`、`http-contract.json`、`svelte-browser.json`、
`browser-performance.json`、`native-linux-x86_64.json`、`windows-core.json`。
性能口径与最终数字见 [性能验收报告](performance.md)。

## 重放

所有数据生成器使用合成日志；不要把用户日志复制进 fixture 或报告。
一次只能运行一个占用 8765 的验证器。验证器识别当前 worktree 的实际监听进程后优雅替换，
结束时恢复当前 worktree 服务；不使用其他 dashboard 端口。

```bash
npm ci --prefix web
npm run check --prefix web
npm test --prefix web
npm run build --prefix web
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
COUSASH_REQUIRE_WEB=1 cargo build --release --examples --bin codex-usage-dashboard
python3 -m unittest discover -s tests -v
python3 tools/compatibility.py
python3 tools/verify_core.py
python3 tools/verify_parser_bridge.py
python3 tools/verify_remote_bridge.py
# 浏览器工具依赖与合成 10k 摘要输入：
npm ci --prefix benchmarks/refactor/frontend
python3 benchmarks/refactor/backend.py generate
# 按 Playwright 提示安装 Chromium 后：
COUSASH_RELEASE=1 COUSASH_BROWSER=1 COUSASH_PERFORMANCE=1 python3 tools/verify_server.py
python3 tools/benchmark_core.py
python3 tools/benchmark_index.py
# 可选 COUSASH_WINDOWS_TEMP 指向 /mnt/c/... 下可用的临时目录：
python3 tools/benchmark_server.py
node tools/package_release.mjs --skip-build
node tools/verify_native.mjs --replace-worktree
```

Windows 原生核心重放：在 WSL 安装 GNU Windows Rust target 和 MinGW 后，运行
`tools/verify_windows_core.py --temp-root /mnt/c/.../Temp`。它将测试可执行文件复制到临时 NTFS 目录，
实际由 Windows 执行，再删除临时文件；不启动另一端口或读取用户日志。

## 发布边界

未验证平台不标完成。CI 原生矩阵已准备但未提交运行；macOS、Windows 原生完整交互/安装及制品验证仍为发布前任务。
候选版使用保守切换策略，保留旧缓存和 v1 快照；运行入口已统一为 Rust。没有实现 Tauri、自动更新器、
自动同步、遥测或日志上传。历史构建验证保留在各报告中；Rust 唯一入口的验证见 `native-only-linux-x64.json`。
