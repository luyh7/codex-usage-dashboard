# Desktop 1.0.1 candidate

Windows 与 Mac 免费直装客户端候选版本，包含匹配的统计程序，不需要安装 Node、Rust、Skill 或平台账号。

首发验收范围为 Windows 11 x64（build 22631、已有 WebView2）和 macOS 26.6.2 Apple Silicon arm64。Intel、旧系统及独立 Linux 桌面不在本次稳定支持范围。

Windows 安装器未购买应用签名证书；Mac 使用有效 ad-hoc 签名，没有 Developer ID 或 Apple 公证。首次打开可能显示 SmartScreen/Gatekeeper 提示，设备管理策略可能禁止运行。请核对下载来源与 SHA256，并按系统针对该应用的提示处理。

客户端通过独立的 GitHub Releases HTTPS 频道检查更新，安装前验证长期公钥对应的更新签名及发行版本。更新替换应用与随包程序，保留业务数据；另提供完整安装包用于手动升级和回退。

两端最终候选已通过全新安装、同版本重装、卸载保留数据、旧完整包回退和 1.0.0 → 1.0.1 覆盖升级；设置、设备身份、导入快照及 24K 合成数据保留。两端从公开 HTTPS 频道发现更新、验证签名、安装重启及数据保留也已通过。Mac 带隔离标记的浏览器下载包会触发 Gatekeeper 拦截，单应用例外后的首次启动仍在验收中；当前 Windows 下载环境没有观察到 SmartScreen，不承诺其他安全策略下无需确认。完整证据与未验证项目见仓库 docs/desktop-review-2026-10-24.md。候选公开预发布与稳定发布分别确认，不因 CI 通过自动转为稳定版。
