"""Generate the human report from measured evidence, not hand-entered timings."""
import json
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
def load(name):return json.loads((HERE/'results'/name).read_text())

def main():
    b=load('backend.json');f=load('frontend.json');io=load('io-query.json');h=load('http.json');c=load('correctness.json');v=load('verification.json')
    assert v['status']=='passed' and c['status']=='passed'
    assert b['environment']['source_sha256']==f['environment']['source_sha256']
    rows={r['name']:r for r in b['results']}
    lines=['# Rust / 前端重构前性能评估','',f"完成时间：{v['completed_at']}。原代码提交：`{b['environment']['commit']}`。",'',
        '**结论：建议重构，但优先做长时间线分页和减少重复计算；Rust 核心采用强类型与借用，保留现有增量及跨盘缓存。不能把模块加速倍数相乘或当作整站加速。**','',
        '## 实验边界','',
        '- 生产实现保持不变；Rust 为可运行的模块原型，尚不是完整替代后端。前端为复用真实分组与 HTML 的局部更新原型，尚不是完整 Svelte 产品。',
        '- 所有数据确定性生成，无真实会话内容。后端 1,000 / 10,000 会话、10,000 时间线事件；完整扫描使用 160 个文件、12,800 token 事件。',
        '- 15 次计时、2 次预热（完整扫描、HTTP、详情等另在原始文件注明）；操作耗时排除进程启动、载入测试 JSON 和结果序列化。typed Rust 返回强类型结果，最终转 JSON 位于计时外，Python 同样只计函数返回。',
        '- 普通 Rust JSON 对象实现会复制/分配对象；typed 实现使用数值字段和借用，二者同时保留用于暴露迁移策略差异。',
        '- OS 文件缓存是热的。“无解析缓存扫描”不代表清空系统页缓存后的首次磁盘读取。P95 是有限样本的近似值，不是稳定 SLA。',
        '- 编码体积、RSS、浏览器堆均单独测量。RSS 包含整个 worker、输入与结果；不代表生产应用常驻内存。浏览器堆不包含完整 WebView / GPU 内存。',
        '- 前端耗时包含 JS、DOM 更新与强制布局；requestAnimationFrame 延迟不等于绘制完成。未测 macOS WKWebView、Windows 原生 WebView2、Tauri 启动或安装包。',
        '- 使用当前 Python 结果作为差分基准，并加入可手算的计费、fork/reset 断言；没有重新认证价格表的外部准确性。','',
        '## 环境','',f"- {v['cpu_model']}，{v['cpu_count']} 个逻辑 CPU；{b['environment']['platform']}。",
        f"- Python `{b['environment']['python'].split()[0]}`；`{b['environment']['rust']}`；SQLite（Python）`{b['environment']['sqlite_python']}`。",
        f"- Chromium `{f['environment']['browser']}`；Svelte `{f['environment']['svelte']}`；1440×1000；时区 Asia/Shanghai。",
        '- 依赖版本固定在 Cargo.lock、package-lock.json；Rust release 构建。','',
        '## 后端模块：同一输入的中位耗时','',
        '| 模块 / 规模 | Python ms | Rust ms | Python ÷ Rust | Rust/Python worker 峰值 MiB |',
        '|---|---:|---:|---:|---:|']
    names={'summary_10000':'汇总 10k（通用 JSON）','summary_typed_10000':'汇总 10k（强类型）','serialize_10000':'JSON 编码 10k 会话',
        'cache_encode_10000':'缓存压缩 10k 会话','cache_decode_10000':'缓存解压 + JSON 解码 10k',
        'price_lookup_10000':'历史价格/模型/档位查询 10k','cost_arithmetic_20001':'费用算术 20k（通用 JSON）','cost_typed_20001':'费用算术 20k（强类型）',
        'timeline_delta_10000':'时间线差分 10k（复制 JSON）','timeline_typed_10000':'时间线差分 10k（借用/强类型）',
        'daily_aggregation_10000':'按日聚合 10k 事件','fork_rebase':'继承前缀匹配 + 用量重算',
        'remote_merge_1000_750':'远程快照 1000 + 750 合并','jsonl_projected_9mb':'JSONL 读取/投影/校验',
        'jsonl_append_32':'追加 32 条日志读取','sqlite_indexed_top50':'同一 SQLite 索引查询 Top 50'}
    for name,label in names.items():
        r=rows[name];py=r['python'];rs=r['rust'];lines.append(f"| {label} | {py['median_ms']:.3f} | {rs['median_ms']:.3f} | {r['speedup_python_over_rust']:.2f}× | {rs['peak_rss_kib']/1024:.1f} / {py['peak_rss_kib']/1024:.1f} |")
    lines += ['', '小于 1× 表示 Rust 原型更慢。通用 JSON 移植的负面结果说明，语言替换不能代替数据结构设计。微秒级 SQLite 查询即使倍数较大，绝对节省也很小。','',
        '读取测试覆盖的是生产 `read_rollout_jsonl` 的输出投影，不包含完整 `parse_file` 的全部元数据、工具统计、runtime tier、续写合并和代理归一化。fork 原型覆盖前缀匹配及 rebase，不覆盖缺失父节点等所有归一化路径。计价查表覆盖历史生效时间、模型别名、长上下文边界、Fast/Ultrafast；完整 price segment 汇总仍是正式迁移验收项。','',
        '## 当前完整后端基线','', '| 路径 | 中位 ms | P95 ms |','|---|---:|---:|']
    for name,r in b['production_baseline'].items():lines.append(f"| `{name}` | {r['median_ms']:.3f} | {r['p95_ms']:.3f} |")
    lines += ['', '以上没有对应完整 Rust 后端，因此不提供端到端 Rust 加速倍数。已有无变化扫描和详情快照查询很快，应保护而不是盲目替换。', '',
        '当前 8765 HTTP 真正启动了生产脚本并使用隔离的合成数据/配置；测试结束恢复默认数据源：','',
        '| HTTP 路径 | 中位 ms | 响应字节 |','|---|---:|---:|']
    for name,r in h['requests'].items():
        if isinstance(r,dict):lines.append(f"| `{name}` | {r.get('median_ms',r.get('ms',0)):.3f} | {r.get('bytes','—')} |")
    lines += ['', 'HTTP 首次列表只有一次样本；并发测试为 4 个客户端、16 次请求。远程导入、重命名、删除及恢复原会话数全部验证通过。', '',
        'cProfile 的含探针耗时不能直接作为请求延迟。调用关系显示 `pricing_for_timeline` 在解析与归一化阶段重复出现；见 [调用栈统计](../../benchmarks/refactor/results/cold-profile.txt)。优化应先保存可复用事件增量，再按受影响的代理关系重算，不能简单删掉第二次计价。','',
        '## WSL 跨盘：语言收益取决于 I/O 策略','',
        '| 位置 / 路径 | Python ms | Rust ms |','|---|---:|---:|']
    for r in io['io']:
        if r.get('unavailable'):continue
        for label,py,rs in [('2000 文件枚举+metadata','python_scandir','rust_read_dir'),('日志读取 / 默认 8 KiB buffer','python_reader','rust_reader'),('日志读取 / Rust 128 KiB buffer','python_reader','rust_reader_128k_buffer')]:
            lines.append(f"| {r['location']}：{label} | {r[py]['median_ms']:.3f} | {r[rs]['median_ms']:.3f} |")
        lines.append(f"| {r['location']}：现有 inventory 活跃文件 | {r['current_inventory_active']['median_ms']:.3f} | 未实现相同缓存模块 |")
        lines.append(f"| {r['location']}：现有 inventory 历史文件缓存 | {r['current_inventory_historical']['median_ms']:.3f} | 未实现相同缓存模块 |")
    lines += ['', '原始枚举、生产 inventory 和缓存命中执行的工作不同，不能互相当成公平的语言对比。历史缓存一行测的是连续请求、2 秒 TTL 未过期时的命中；不代表间隔 10 秒自动刷新时仍有同样收益。Windows 盘上的元数据缓存、批量读取和轮询策略是必保能力；本结果不是 Windows 原生性能。', '',
        '## 查询架构实验','']
    q=io['query'];lines += [f"10k 会话筛选排序 Top 50：Python 内存遍历 `{q['python_in_memory_top50']['median_ms']:.3f} ms`；同语言 SQLite 索引查询 `{q['python_indexed_top50']['median_ms']:.3f} ms`；一次性内存索引构建 `{q['index_build_ms']:.3f} ms`。",
        f"完整会话 JSON `{q['full_sessions_json_bytes']:,}` 字节，50 条会话 JSON `{q['page50_sessions_json_bytes']:,}` 字节。",
        '', '这组收益来自查询和传输范围变化。索引构建使用内存 SQLite，未证明磁盘持久索引更新吞吐；筛选仅 project equality，不等于全部生产筛选、父子代理树和自定义日期语义。','',
        '## 前端：相同业务分组、相同可见内容','',
        '| 会话数 / 操作 | 当前实现 ms | 原生 keyed DOM ms | Svelte keyed ms |','|---|---:|---:|---:|']
    for size in [1000,10000]:
        for scenario in ['unchanged','search','one_session_update','project_toggle','agent_toggle']:
            values={r['variant']:r['median_ms'] for r in f['results'] if r['module']=='table' and r['size']==size and r['scenario']==scenario}
            lines.append(f"| {size} / {scenario} | {values['legacy']:.2f} | {values['keyed_dom']:.2f} | {values['svelte']:.2f} |")
    lines += ['', '**`unchanged` 是强制重绘微基准。现有自动刷新收到 204 已经不会重绘，不能将这一行宣称为真实空闲刷新收益。**', '',
        '原型复用项目/代理分组和格式化函数，仅替换 DOM 更新方式；候选使用事件委托。初始与变更后的会话 ID、汇总、分组、可见文本一致，真实点击可折叠项目。表格每项目预览 5 个任务，并没有把 10k 行全部插入 DOM。','',
        'Svelte 与原生局部更新相近；当前证据支持“局部更新”，不能证明 Svelte 比 React/Vue 更快。筛选时仍需重做分组和文本格式化，因此仅换渲染框架提升有限。','',
        '| 独立前端模块 | 中位 ms | P95 ms |','|---|---:|---:|']
    for r in f['results']:
        if r['module']=='isolated_ui':lines.append(f"| `{r['name']}` | {r['median_ms']:.2f} | {r['p95_ms']:.2f} |")
    lines += ['', '详情分页实验保留原元数据和完整曲线，只渲染最新 100 个事件。它改变了首屏工作量；并非 Rust 或 Svelte 的语言加速。分页导航、服务端分页、索引构建和完整导出要在正式实现中补齐。搜索预索引保留同一文本字段，但本实验只验证其他筛选均为 all 时的文本搜索。', '',
        '日历、远程管理弹窗和 10k 点曲线绘制已经较快，此轮不值得单独迁移为 WASM；前端 Rust/WASM 未实现也未测量。', '',
        '| 浏览器 JS 堆（强制 GC 后） | 当前 MiB | keyed DOM MiB | Svelte MiB |','|---|---:|---:|---:|']
    for size in [1000,10000]:
        values={r['variant']:r['used_bytes']/1024**2 for r in f['results'] if r['module']=='browser_heap' and r['size']==size}
        lines.append(f"| {size} 会话 | {values['legacy']:.2f} | {values['keyed_dom']:.2f} | {values['svelte']:.2f} |")
    lines += ['', '## 正确性与覆盖','',
        f"- 生产 Python 测试 {v['production_tests']} 项通过；npm 安装器语法检查通过。",
        f"- 后端 {len(b['results'])} 项性能用例逐项核对；83 组额外差分/手算测试通过，其中历史价格边界组合 {c['price_grid_cases']:,} 个。",
        '- 覆盖：BOM、非法 UTF-8、坏行、未完成尾行、忽略大输出、JSON 属性顺序变化、乱序事件、计数器重置、缓存字段别名、非法字段类型、历史价格、前缀去重、未知价格、远程替换 UID、压缩和序列化往返。',
        '- 整数逐项精确比较；浮点相对误差 1e-12、绝对误差 1e-8。JSON 字段顺序和压缩后的字节大小不要求相同。',
        '- Rust 原型仍有明确限制：时间格式支持范围小于 Python fromisoformat；有符号 64 位计数；按日统计固定 Asia/Shanghai；完整 fork 状态、runtime tier、git worktree 与所有跨平台异常尚未迁移。',
        '- 原始输出和源码校验值见 [verification.json](../../benchmarks/refactor/results/verification.json)。','',
        '## 模块处置建议','',
        '| 模块 | 本轮证据 | 重构决策 |','|---|---|---|',
        '| 日志读取与增量续读 | Rust 模块 + 边界差分 + 本地/跨盘 | 迁移，保留文件身份、偏移、尾行和 buffer 策略 |',
        '| 完整会话解析、续写与代理归一化 | 现有完整测试 + profile；Rust 仅部分核心 | 先补跨语言 golden fixture，不能直接宣布等价 |',
        '| 历史计价 / 用量聚合 | 差分 + 正负性能对照 | 强类型；事件增量复用；保留 unknown |',
        '| SQLite 缓存 / 索引 | codec + 同 SQL + 查询形态对照 | 保留现有快路径，索引与快照事务一致性需新测试 |',
        '| 远程快照 | merge 差分 + 生产 HTTP 完整往返 | 保留 v1 格式和设备命名空间 |',
        '| HTTP、并发、导出 | 生产基线，无完整 Rust server | 以兼容合同和后续 E2E 测试验收 |',
        '| 搜索 / 项目树 / 详情 | Chromium 真 DOM 对照 | 先分页、搜索索引和派生数据复用，再组件化 |',
        '| 日历 / 图表 / 远程弹窗 | 生产前端基线 | 保留语义，随 UI 拆组件；无必要上 WASM |',
        '| git 元数据 / 启动器 / Tauri / 发布 | 功能测试或未实现；无跨平台性能结果 | 保留行为，平台发布前实机门禁 |','',
        '实施顺序和逐步验收见 [agent 重构方案](../plans/rust-svelte-refactor.md)。','',
        '## 复现','',
        '在仓库根目录运行：','',
        '```bash','python3 benchmarks/refactor/run_all.py','```','',
        '需要 Python、Rust/Cargo、Node/npm、ss，以及本机 Playwright Chromium 缓存。Windows 挂载目录通过当前实验环境路径检测，不存在时明确记录 unavailable。脚本顺序执行，使用临时文件，HTTP 测试固定 8765，最后恢复当前工作区默认 dashboard。',
        '', '生产代码未改动；报告中的候选收益属于预重构实测，完整 Rust + 新前端 + 桌面客户端的端到端结果必须在正式重构完成后补测。','']
    path=ROOT/'docs/performance/refactor-evaluation.md';path.parent.mkdir(parents=True,exist_ok=True);path.write_text('\n'.join(lines))
    print(path)

if __name__=='__main__':main()
