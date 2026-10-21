# Clear Mint / A · 清透薄荷

当前唯一主题。原稿由内置 imagegen 生成，使用虚构演示数据。此目录保存未经修改的 PNG 原图，作为 Svelte 界面的视觉基准。

| 状态 | 设计稿 | 原始提示词 |
| --- | --- | --- |
| 概览 | [overview.png](overview.png) | [overview-prompt.txt](overview-prompt.txt) |
| 日历展开 | [calendar-open.png](calendar-open.png) | [calendar-projects-prompts.txt](calendar-projects-prompts.txt) |
| 项目页面 | [projects.png](projects.png) | [calendar-projects-prompts.txt](calendar-projects-prompts.txt) |

三张稿件均为 1536 × 1024。项目页是最新的共同样式基准；日历稿定义右上角浮层的位置、范围选择和底部操作。

## 实现基准

- 160px 侧栏；桌面主区左间距 28px，右间距 23px；卡片圆角 15–16px。
- 冷白背景 `#f6fafc`、白色面板、深蓝黑正文 `#16263f`、标题 `#08151f`、次要文字 `#617593`。
- 主色 `#00a469`、费用深绿 `#004a37`、选中背景 `#e8faf2`，细边框 `#e3ecf4`。
- 项目页采用不等宽统计卡和约 55:45 的列表/详情布局；日历为 712px 浮层，展开时不移动页面内容。
- 字体由本地提供的 Dashboard Sans 覆盖英文、常用简体中文和标点，来源为 Noto Sans CJK SC 的子集。许可见 [OFL.txt](../../../web/src/assets/fonts/OFL.txt)。其他字符回退到系统中文无衬线字体。
- 任务合计只用于展开分组；选中的主任务自身或子代理与右侧详情保持同一统计范围。已完成的单页列表不显示无效分页按钮。
- 主任务详情保留两项折叠入口：用量事件、更多信息。原有消息、工具、计价区间、环境和元数据可在更多信息中查看。
- 日历日期选择先保留为草稿，应用后才更新查询；支持取消、点击外部、Escape、键盘选择、跨月范围和输入起止日期。
- 桌面按照原稿校准，窄屏将详情与日历改为可操作的浮层，并保留减少动画的系统偏好。

## 数据与稿件的区别

实际页面显示真实数据，因此不显示“演示数据”、虚构增长率或装饰性用量曲线。项目页最新稿件中的简洁统计卡用于统一主题；本周、本月结束日期遵循 API，统计到当天。日历的更多选项保留已有近 30 天和全部时间功能。

浏览器校验用 [clear_mint.mjs](../../../tools/fixtures/clear_mint.mjs) 提供原稿中的项目、费用和用量，避免修改用户日志或快照。验证截图保存在 `output/verification/clear-mint/`，不替代本目录中的原始设计稿。
