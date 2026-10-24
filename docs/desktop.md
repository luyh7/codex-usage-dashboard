# 桌面客户端

在桌面查看 Codex 用量、费用和会话统计。

## 下载

在[发布页](https://github.com/luyh7/codex-usage-dashboard/releases/latest)选择 Windows（64 位）或 macOS（Apple Silicon）的安装包。

## 安装

- **Windows**：运行 `.exe`，按提示完成安装，从开始菜单打开。
- **Mac**：打开 `.dmg`，将应用拖入 Applications，再打开应用。

客户端会自动发现本机 Codex 日志。需要使用其他目录时，在“设置”中选择日志文件夹。

## 使用

- 在概览中查看用量、费用和会话，按日期、项目或模型筛选。
- 在“设置”中切换语言和展示时区。
- 在设备数据中导入、导出快照，汇总其他设备的用量。

## 更新与回退

当前版本通过下载安装包手动更新，原有设置和导入数据会保留。

如需回退，先退出客户端，卸载时保留数据，再安装 [1.0.1](https://github.com/luyh7/codex-usage-dashboard/releases/tag/v1.0.1)。

## 遇到问题

- **没有显示用量**：在“设置”中确认日志目录，并检查目录内是否有 Codex 会话日志。
- **首次打开出现系统安全提示**：确认文件来自本仓库后，参考 [Mac 打开应用说明](https://support.apple.com/zh-cn/guide/mac-help/mh40616/mac)或 [Windows 安装提示说明](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)。
- **仍然无法使用**：在 [Issues](https://github.com/luyh7/codex-usage-dashboard/issues) 中提供系统信息和复现步骤。

[命令行与 Skill 用法](usage.md) · [技术与验收记录](implementation/desktop-client.md)
