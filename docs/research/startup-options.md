# 启动体验方案调研

调研日期：2026-10-21。范围：官方文档与当前仓库源码；没有做用户访谈、市场占有率统计或安装实测。下文的“事实”与“适用性判断”分开记录。命令名 `codex-usage` 仅作为建议示例，尚未实现或发布。

## 当前项目事实

- README 主推 `npx skills add ...` 安装，然后重启 Codex/新建对话，再从 `/usage` 选择技能。独立安装入口 `npx github:luyh7/codex-usage-dashboard` 默认只复制技能；打开和创建图标需要分别添加 `--open`、`--shortcut`。见 [README](../../README.md)、[安装器](../../bin/install.js)。
- npm 的唯一 `bin` 为 `codex-usage-dashboard-skill`，指向安装器，没有独立的日常启动命令；即使使用 `--open` 也先复制技能。安装器使用 Node >=18，真正运行仪表盘还要查找 Python。见 [package.json](../../package.json)、[安装器](../../bin/install.js)。
- 安装器只用 `--version` 的退出码检测 Python，不检查最低版本；当前仪表盘顶层使用 `datetime.UTC`，因此实际至少需要 Python 3.11（[Python 官方说明](https://docs.python.org/3.11/library/datetime.html#datetime.UTC)）。缺 Python 只打印警告，`--open/--shortcut` 子进程的非零状态也没有向上传播。见 [安装器](../../bin/install.js)、[仪表盘](../../skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py)。
- Python 启动器已经实现：探测 `127.0.0.1:8765/api/health`，命中时直接开浏览器，否则后台启动并等待就绪，再开浏览器；Windows 使用隐藏窗口，其他平台使用独立 session，日志写入临时目录。启动器公开参数只有 `--host`，没有 `status/stop/restart`。见 [启动器](../../skills/codex-usage-dashboard/scripts/open_dashboard.py)。
- 健康检查依靠一长串功能标记识别可复用的服务，没有代码版本、构建标识或源码路径；因此升级后若功能标记未改变，仍可能复用旧进程。这是从启动器判断条件得出的适用性风险，而非已经复现的用户故障。
- 桌面图标当前只实现 Windows `.lnk` 与 macOS `.command`；Linux（包括通常被识别为 Linux 的 WSL）返回不支持。图标依赖安装目录内脚本与 Python。见 [快捷方式安装器](../../skills/codex-usage-dashboard/scripts/install_desktop_shortcut.py)。

## 可借鉴的五类成熟做法

| 实例/模式 | 官方事实：首次安装与日常使用 | 进程与更新 | 对本项目的适用性判断 |
| --- | --- | --- | --- |
| Streamlit：短命令打开本地网页 | `pip install streamlit` 安装；`streamlit run app.py` 启动本地服务器并自动打开默认浏览器。也支持 `python -m streamlit run`。[安装](https://docs.streamlit.io/get-started/installation)、[运行](https://docs.streamlit.io/develop/concepts/architecture/run-your-app) | 这是启动本地 Python 服务的入口；上述文档没有声称桌面安装或系统服务托管。 | 借鉴“记一个短命令即可打开”；本项目现有浏览器自动打开和后台启动已经覆盖核心部分，无须为了入口迁移到 Streamlit。 |
| JupyterLab / Jupyter Server：可管理的本地服务 | `jupyter lab` 自动开浏览器，也能手动访问服务 URL。[启动](https://jupyterlab.readthedocs.io/en/stable/getting_started/starting.html) | Server 明确提供 `list`、`stop` 子命令，支持按端口停止，列表支持 JSON。[官方 API 文档](https://jupyter-server.readthedocs.io/en/stable/api/jupyter_server.html) | 值得优先借鉴：`open/status/stop/restart/logs` 使后台进程的状态和退出方式可发现。关闭网页与停止后台服务应在产品文案中交代清楚。 |
| uv tool / uvx 与 pipx：Python 应用分发 | `uvx` 在隔离临时环境执行；`uv tool install` 持久安装并把命令放到 PATH；pipx 也为每个应用创建独立环境，并支持 console script 入口。[uv 使用指南](https://docs.astral.sh/uv/guides/tools/)、[pipx](https://pipx.pypa.io/stable/) | uv 有 `tool upgrade`；uvx 环境会缓存，不应理解为每次运行必取最新版。uv 在缺少所需 Python 时可自动下载。[uv 环境与版本](https://docs.astral.sh/uv/concepts/tools/)、[Python 管理](https://docs.astral.sh/uv/guides/install-python/) | 若整理成标准 Python 包，可提供 `uvx codex-usage` 与持久短命令，避免“Node 安装器 + 手工 Python 路径”。但用户仍先需要 uv 或 pipx；包管理器负责环境，不负责服务器的状态/退出，也不能代替桌面入口。 |
| npm npx：包即命令 | `npx` 基于 `npm exec`；可执行本地或远程包，缺失包放进 npm 缓存并加入执行进程 PATH，通常先提示安装；单个 `bin` 可作为默认执行入口。[npm 官方文档](https://docs.npmjs.com/cli/v11/commands/npm-exec/) | 支持缓存、版本指定与在线/离线选项；这不是进程托管功能。[同一官方文档](https://docs.npmjs.com/cli/v11/commands/npm-exec/) | 对已有 Node 用户，最低改造成本是让默认 `npx ...` 直接打开，把 `install-skill`、`shortcut` 作为明确动作。当前 bin 名含 `-skill` 且默认只安装，用户意图与结果之间多了一层。若仍靠 Python，必须明确检测与解决依赖；单靠 npx 不能消除 Python 前提。 |
| Ollama：原生应用 + 托盘/菜单栏 + 后台服务 | Windows 安装后在后台运行，可从开始菜单重开、托盘退出；macOS 用 DMG 拖入 Applications。[Windows](https://docs.ollama.com/windows)、[macOS](https://docs.ollama.com/macos) | macOS/Windows 自动下载更新，用户点 Restart to update 生效；Linux 可用 systemd 托管并以安装脚本更新。[FAQ](https://docs.ollama.com/faq)、[Linux](https://docs.ollama.com/linux) | 最值得借鉴的是系统图标入口、可见运行状态与退出/重启操作。完整原生应用、自动升级与各平台服务会增加打包、签名、升级和维护工作；是否值得应由目标用户规模与使用频率决定。 |

## 跨平台与 WSL

**官方事实：** 微软说明 Windows 浏览器通常可用 `localhost` 访问 WSL 内服务器；Windows 可通过 `wsl.exe --distribution <DistroName> ...` 执行特定发行版命令。反向从 WSL 访问 Windows 服务在 NAT 与 mirrored 模式下规则不同。[网络文档](https://learn.microsoft.com/windows/wsl/networking)、[互操作文档](https://learn.microsoft.com/en-us/windows/dev-environment/wsl-interop)

**适用性判断：** WSL 用户可采用“Windows 开始菜单/桌面入口 → 指定 WSL 发行版 → 项目启动器 → Windows 浏览器”的路径。需要显式保存或选择发行版，并测试冷启动、Python 所在环境、浏览器打开和两侧端口占用；不能把 Linux `.desktop` 当作 Windows 启动入口。若原生 Windows 运行，也要验证读取 WSL 日志的方式；现有 WSL 自动合并 Windows 日志不等于反向也成立。

## 建议优先级（产品与工程判断）

1. **先统一日常动作。** 一个 `codex-usage` 命令/系统图标完成“已运行则打开，未运行则启动再打开”；技能仍调用同一入口。安装是首次动作，不应要求用户每天启动 Codex 对话才能看仪表盘。
2. **补齐后台可控性。** 提供状态、停止、重启、查看日志与 `doctor` 诊断，检查实际 Python 版本并正确返回错误；识别本项目自己的进程与代码版本后再处理旧实例；仍固定使用 8765。多个入口应汇合到同一套逻辑，避免图标、CLI、技能各自启动不同实例。
3. **按用户群选择一种主分发。** 近期可先复用 npm 安装通道，优先解决默认动作、短命令和快捷方式；若主群体是 Python/终端用户，再补标准 Python 包和 uv/pipx，不把新的包管理器作为所有人的默认前提。非技术用户较多时，再评估携带运行时的系统安装包：例如 PyInstaller 保留 Python 实现，用户无需自行安装 Python，但需要分别在 Windows/macOS/Linux 构建，并验证打包后服务子进程的启动路径。[PyInstaller 官方说明](https://www.pyinstaller.org/en/stable/)
   打包边界：现有启动器用 `[sys.executable, script.py]` 启动后台子进程，仪表盘还使用 `spawn` 多进程且没有 `freeze_support`；需要适配冻结后的入口和子进程，不能假设现有脚本直接打包便能正常运行。见 [启动器](../../skills/codex-usage-dashboard/scripts/open_dashboard.py)、[仪表盘](../../skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py)。
4. **按需评估常驻。** 图标按需启动已经可大幅减少操作；托盘和开机启动可以后置。浏览器书签适合作为运行后的附加入点，浏览器本身不能唤起一个尚未运行的普通本地 HTTP 服务。

上述建议尚未实施。此文件只新增调研记录，没有修改运行代码或启动服务。
