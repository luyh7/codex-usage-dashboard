# Clear Mint 设计记录

本页保存来源与历史决策。当前视觉参考见 [README](README.md)，交互要求见 [UI 契约](../../development/ui-contract.md)。

## 稿件来源

原稿由内置 imagegen 生成，使用虚构演示数据。本目录保留未经修改的 PNG 原图。

| 稿件 | 原始提示词 |
| --- | --- |
| [概览](overview.png) | [overview-prompt.txt](overview-prompt.txt) |
| [日历](calendar-open.png) | [calendar-projects-prompts.txt](calendar-projects-prompts.txt) |
| [项目列表](projects-from-overview.png) | [projects-from-overview-prompt.txt](projects-from-overview-prompt.txt) |

## 已采用的决策

2026-10-22：

- 以概览稿建立共同视觉基准，项目列表重新从概览派生。[早期项目稿](projects.png)未采用，仅供溯源。
- 统计卡采用组合方案：总用量取「趋势洞察」的每日用量；会话数取「用量构成」的主会话 / 子代理构成；费用保留累计曲线，缓存使用纯色圆环。候选图稿与数据来源见 [统计卡方案记录](metric-options/README.md)。

## 早期验收素材

[verify_mint_ui.mjs](../../../tools/verify_mint_ui.mjs) 和 [verify_overview_layout.mjs](../../../tools/verify_overview_layout.mjs) 记录了原稿还原与固定布局的验收流程，截图目录分别为 `output/verification/clear-mint/` 和 `output/verification/overview-layout/`。

这些脚本含固定的 8765 地址及早期页面选择器，复用时需先对照当前工作树的 URL 和界面结构。浏览器依赖可通过 `PLAYWRIGHT_MODULE`、`PLAYWRIGHT_EXECUTABLE_PATH` 指定。当前验收要求见 [UI 契约](../../development/ui-contract.md#verification)。
