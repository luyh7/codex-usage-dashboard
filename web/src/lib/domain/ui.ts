import type { Query, Usage, Session } from '../data/types';
export type Language = 'zh' | 'en';
export const dictionaries = {
  zh: {
    title: 'Codex 用量',
    subtitle: '本地会话与手动导入快照',
    tokens: '总 tokens',
    cost: '预估费用',
    sessions: '会话数',
    cache: '缓存命中率',
    breakdown: '用量细分',
    input: '输入',
    cached: '缓存输入',
    output: '输出',
    reasoning: '推理输出',
    cacheWrite: '缓存写入',
    unknown: '未知',
    partial: '含未知费用',
    search: '搜索会话、项目、模型…',
    all: '全部',
    today: '今天',
    week: '本周',
    month: '本月',
    custom: '自定义日期',
    days7: '近 7 天',
    days30: '近 30 天',
    environment: '环境',
    source: '状态',
    active: '活跃',
    archived: '已归档',
    model: '模型',
    agents: '代理',
    main: '主会话',
    onlyAgents: '仅子代理',
    sort: '排序',
    updated: '最近更新',
    name: '名称',
    ascending: '升序',
    descending: '降序',
    projects: '项目',
    conversations: '会话',
    filters: '筛选',
    clear: '清除筛选',
    calendar: '日历',
    from: '开始',
    to: '结束',
    apply: '应用',
    previous: '上一页',
    next: '下一页',
    loading: '加载中…',
    indexing: '正在建立索引',
    ready: '已更新',
    retry: '重试',
    empty: '没有符合条件的会话',
    details: '会话详情',
    close: '关闭',
    events: 'Token 事件',
    curve: '累计 tokens 曲线',
    fullCurve: '完整曲线，无下采样',
    prompt: '首条用户消息',
    response: '最近回复',
    tools: '工具调用',
    prices: '计价区间',
    metadata: '会话信息',
    path: '日志路径',
    duration: '任务耗时',
    turns: '轮次',
    inherited: '已排除继承用量',
    unresolved: '父会话尚未找到',
    remote: '远端快照',
    import: '导入 JSON',
    export: '导出本机快照',
    rename: '改名',
    delete: '删除',
    deleteConfirm: '确认删除这份已导入快照？本机日志不受影响。',
    label: '显示名称',
    currentConfirm: '这是当前设备的快照。仍要作为远端副本导入？',
    confirm: '确认',
    cancel: '取消',
    theme: '主题',
    system: '跟随系统',
    light: '浅色',
    dark: '深色',
    density: '密度',
    comfortable: '舒适',
    compact: '紧凑',
    language: '语言',
    expandAgents: '展开代理',
    collapseAgents: '收起代理',
    ownUsage: '主会话自身用量',
    taskTotal: '任务合计',
    show: '显示',
    of: '共',
    selected: '已选择',
    localOnly: '只读本地日志',
    timezone: '显示时区',
    copy: '复制',
    copied: '已复制',
    year: '年',
    monthPicker: '月',
    noEvents: '该范围没有 Token 事件',
    indexError: '索引暂时不可用',
    remoteEmpty: '尚未导入远端快照',
    imported: '已导入',
    count: '条',
    allModels: '全部模型',
    allEnvironments: '全部环境',
    dateHint: '点击开始与结束日期，再应用。',
    estimatedHint: '按公开价格估算；不代表实际账单。',
    unknownHint: '未知模型或档位未计入已知费用。',
    detailsMissing: '此快照没有保存该会话的详情。',
  },
  en: {
    title: 'Codex usage',
    subtitle: 'Local sessions and manually imported snapshots',
    tokens: 'Total tokens',
    cost: 'Estimated cost',
    sessions: 'Sessions',
    cache: 'Cache hit rate',
    breakdown: 'Usage breakdown',
    input: 'Input',
    cached: 'Cached input',
    output: 'Output',
    reasoning: 'Reasoning output',
    cacheWrite: 'Cache writes',
    unknown: 'Unknown',
    partial: 'Includes unknown costs',
    search: 'Search sessions, projects, models…',
    all: 'All',
    today: 'Today',
    week: 'This week',
    month: 'This month',
    custom: 'Custom dates',
    days7: 'Last 7 days',
    days30: 'Last 30 days',
    environment: 'Environment',
    source: 'Status',
    active: 'Active',
    archived: 'Archived',
    model: 'Model',
    agents: 'Agents',
    main: 'Main sessions',
    onlyAgents: 'Subagents only',
    sort: 'Sort',
    updated: 'Recently updated',
    name: 'Name',
    ascending: 'Ascending',
    descending: 'Descending',
    projects: 'Projects',
    conversations: 'Sessions',
    filters: 'Filters',
    clear: 'Clear filters',
    calendar: 'Calendar',
    from: 'From',
    to: 'To',
    apply: 'Apply',
    previous: 'Previous',
    next: 'Next',
    loading: 'Loading…',
    indexing: 'Building index',
    ready: 'Updated',
    retry: 'Retry',
    empty: 'No sessions match these filters',
    details: 'Session details',
    close: 'Close',
    events: 'Token events',
    curve: 'Cumulative token curve',
    fullCurve: 'Full curve, no downsampling',
    prompt: 'First user message',
    response: 'Latest response',
    tools: 'Tool calls',
    prices: 'Pricing segments',
    metadata: 'Session information',
    path: 'Log path',
    duration: 'Task duration',
    turns: 'Turns',
    inherited: 'Inherited usage excluded',
    unresolved: 'Parent session not found yet',
    remote: 'Remote snapshots',
    import: 'Import JSON',
    export: 'Export this device',
    rename: 'Rename',
    delete: 'Delete',
    deleteConfirm: 'Delete this imported snapshot? Local logs are unaffected.',
    label: 'Display name',
    currentConfirm: 'This snapshot belongs to the current device. Import a remote copy anyway?',
    confirm: 'Confirm',
    cancel: 'Cancel',
    theme: 'Theme',
    system: 'System',
    light: 'Light',
    dark: 'Dark',
    density: 'Density',
    comfortable: 'Comfortable',
    compact: 'Compact',
    language: 'Language',
    expandAgents: 'Expand agents',
    collapseAgents: 'Collapse agents',
    ownUsage: 'Main session own usage',
    taskTotal: 'Task total',
    show: 'Showing',
    of: 'of',
    selected: 'Selected',
    localOnly: 'Local logs are read only',
    timezone: 'Display timezone',
    copy: 'Copy',
    copied: 'Copied',
    year: 'Year',
    monthPicker: 'Month',
    noEvents: 'No token events in this range',
    indexError: 'Index temporarily unavailable',
    remoteEmpty: 'No remote snapshots imported',
    imported: 'Imported',
    count: 'rows',
    allModels: 'All models',
    allEnvironments: 'All environments',
    dateHint: 'Choose a start and end date, then apply.',
    estimatedHint: 'Estimated using published prices; not an actual bill.',
    unknownHint: 'Unknown models or tiers are excluded from known costs.',
    detailsMissing: 'This snapshot does not contain session details.',
  },
} as const;
export type Words = typeof dictionaries.zh | typeof dictionaries.en;
export const words = (lang: Language): Words => dictionaries[lang];
export const number = (value: number | undefined | null, lang: Language = 'zh') =>
  new Intl.NumberFormat(lang === 'zh' ? 'zh-CN' : 'en-US', { maximumFractionDigits: 1 }).format(
    value ?? 0,
  );
export const money = (value: number | null | undefined, lang: Language = 'zh') =>
  value == null
    ? words(lang).unknown
    : new Intl.NumberFormat('en-US', {
        style: 'currency',
        currency: 'USD',
        minimumFractionDigits: 2,
        maximumFractionDigits: 6,
      }).format(value);
export const zero = (): Usage => ({
  input_tokens: 0,
  cached_input_tokens: 0,
  cache_write_tokens: 0,
  output_tokens: 0,
  reasoning_output_tokens: 0,
  total_tokens: 0,
});
export const dateTime = (value: string | null | undefined, zone: string, lang: Language) => {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(+date)
    ? value
    : new Intl.DateTimeFormat(lang === 'zh' ? 'zh-CN' : 'en-US', {
        dateStyle: 'short',
        timeStyle: 'medium',
        timeZone: zone,
      }).format(date);
};
export const models = (row: Session) =>
  row.models?.length ? row.models : [row.model || 'unknown'];
export const defaultQuery = (): Query => ({
  period: 'today',
  start: '',
  end: '',
  timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
  search: '',
  model: '',
  source: '',
  environment: '',
  agents: '',
  sort: 'tokens',
  ascending: false,
});
const keys = Object.keys(defaultQuery()) as Exclude<keyof Query, 'offset'>[];
export function fromUrl() {
  const params = new URLSearchParams(location.search);
  const query = defaultQuery();
  for (const key of keys) {
    const value = params.get(key);
    if (value !== null) {
      if (key === 'ascending') query.ascending = value === 'true';
      else query[key] = value;
    }
  }
  return {
    query,
    selected: params.get('session') || '',
    mode: params.get('view') === 'sessions' ? ('sessions' as const) : ('projects' as const),
  };
}
export function saveUrl(query: Query, selected: string, mode: string, replace = false) {
  const params = new URLSearchParams();
  for (const key of keys) {
    const value = query[key];
    if (value !== '' && value !== false) params.set(key, String(value));
  }
  if (selected) params.set('session', selected);
  if (mode !== 'projects') params.set('view', mode);
  const url = location.pathname + '?' + params;
  if (url !== location.pathname + location.search)
    history[replace ? 'replaceState' : 'pushState']({}, '', url);
}
export function stored<T>(key: string, fallback: T): T {
  try {
    return JSON.parse(localStorage.getItem('cousash:v2:' + key) || 'null') ?? fallback;
  } catch {
    return fallback;
  }
}
export function save(key: string, value: unknown) {
  try {
    localStorage.setItem('cousash:v2:' + key, JSON.stringify(value));
  } catch {
    /* private storage may be unavailable */
  }
}
