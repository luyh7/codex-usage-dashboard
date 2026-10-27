// Fictional data from docs/design/clear-mint/projects.png. Never writes user data.
export const stamp = '2026-10-22T01:00:00Z';
export const localDeviceInfo = {
  device_short_code: 'this-device', label: 'Local workstation', platform: 'Windows', session_count: 123,
  sources: [{ id: 'windows', label: 'Windows', session_count: 23 }, { id: 'wsl', label: 'WSL', session_count: 100 }],
};
export const usage = (n = 4200000) => ({ total_tokens: n, input_tokens: n * .95, cached_input_tokens: n * .95 * .89, output_tokens: n * .05, reasoning_output_tokens: n * .02, cache_write_tokens: 0 });
export const session = (uid, title, tokens, cost, project = 'web-app', parent = '') => ({
  uid, session_id: uid, root_session_id: parent || uid, parent_thread_id: parent,
  is_subagent: !!parent, title, agent_nickname: parent ? title : '', source: uid === 'docs-guide' ? 'archived' : 'active',
  environment: 'Local', environment_id: 'local', project, project_root: '/demo/' + project, cwd: '/demo/' + project,
  model: 'GPT-6.1 Sol', models: ['GPT-6.1 Sol'], service_tier: 'standard', effort: 'high',
  created_at: stamp, updated_at: stamp, end_at: stamp, last_usage_at: stamp, total_token_usage: usage(tokens),
  estimated_cost_usd: cost, price_model_known: cost !== null, turn_count: 6, completed_turn_count: 6, duration_ms_total: 1800000,
  inherited_token_event_count: 0, inherited_token_usage: usage(0), fork_usage_resolved: true,
  estimated_cost_breakdown_usd: cost === null ? null : uid === 'main' ? { input_tokens: 2.10, cached_input_tokens: 1.26, output_tokens: 3.12, reasoning_output_tokens: 0 } : { input_tokens: cost / 3, cached_input_tokens: cost / 6, output_tokens: cost / 2, reasoning_output_tokens: cost / 4 },
  first_user_prompt: 'A fictional conversation for design verification.', last_agent_preview: 'Done.',
  tool_counts: { shell: 3 }, applied_price_segments: [], path: '/demo/session.jsonl',
});
export const sessions = [
  session('main', '重构登录流程', 4200000, 6.48),
  session('agent-a', '检查登录状态', 620000, 1.02, 'web-app', 'main'),
  session('agent-b', '补充登录测试', 380000, .62, 'web-app', 'main'),
  session('layout', '修复移动端布局', 2100000, 2.76),
  session('styles', '更新组件样式', 1100000, 1.32),
  session('query', '优化数据查询', 1500000, 2.20, 'data-tools'),
  session('cleanup', '整理导入数据', 1000000, 1.10, 'data-tools'),
  session('index', '优化查询索引', 600000, 1.08, 'data-tools'),
  session('docs', '更新文档导航', 800000, 1.32, 'docs-site'),
  session('docs-guide', '补充使用指南', 540000, .72, 'docs-site'),
];
export const unknown = session('unknown', '未知模型费用', 4200000, null);
export const summary = rows => {
  const tokens = rows.reduce((n, r) => n + r.total_token_usage.total_tokens, 0);
  return { session_count: rows.length, main_count: rows.filter(r => !r.is_subagent).length, agent_count: rows.filter(r => r.is_subagent).length, active_count: rows.filter(r => r.source === 'active').length, archived_count: rows.filter(r => r.source === 'archived').length, usage: usage(tokens), estimated_cost_known_count: rows.filter(r => r.estimated_cost_usd != null).length, estimated_cost_usd: rows.reduce((n, r) => n + (r.estimated_cost_usd || 0), 0), by_model: [{ model: 'GPT-6.1 Sol', sessions: rows.length, usage: usage(tokens) }], by_environment: [] };
};
export async function installMintFixture(context, { language = 'zh', storedTheme = 'dark' } = {}) {
  let remotes = [], writes = 0;
  await context.addInitScript(({ language, storedTheme }) => {
    localStorage.setItem('cousash:v2:language', JSON.stringify(language));
    localStorage.setItem('cousash:v2:theme', JSON.stringify(storedTheme));
    if (!localStorage.getItem('cousash:v2:projects:this-device')) localStorage.setItem('cousash:v2:projects:this-device', '["web-app"]');
    if (!localStorage.getItem('cousash:v2:agents:this-device')) localStorage.setItem('cousash:v2:agents:this-device', '["main"]');
  }, { language, storedTheme });
  await context.route('**/api/**', async route => {
    const request = route.request(), url = new URL(request.url()), p = url.searchParams, path = url.pathname;
    if (path === '/api/health') return route.continue();
    const json = body => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
    if (path === '/api/revisions') return route.fulfill({ status: 200, contentType: 'text/event-stream', body: 'event: revision\ndata: {"phase":"ready","revision":1}\n\n' });
    const rows = sessions.filter(row => (!p.get('search') || (row.title + row.project + row.model).includes(p.get('search'))) && (!p.get('source') || p.get('source') === row.source) && (!p.get('model') || p.get('model') === row.model) && !(p.get('period') === 'custom' && p.get('start')?.startsWith('2010')));
    const s = summary(rows), revision = 'clear-mint-design-v2';
    const page = items => ({ query_token: 'query', revision, items, summary: s, total: items.length, offset: 0, limit: 100, next_cursor: null, previous_cursor: null });
    const metricTrend = { complete: true, points: rows.length ? [1, 2, 3, 4].map((weight, i) => ({ date: '2026-10-' + (19 + i), usage: usage(s.usage.total_tokens * weight / 10), session_count: rows.length, estimated_cost_usd: s.estimated_cost_known_count === rows.length ? s.estimated_cost_usd * weight / 10 : null })) : [] };
    const listing = items => ({ metric_trend: metricTrend, page: page(items), facets: summary(sessions), period: { key: p.get('period'), start_date: p.get('period') === 'custom' ? p.get('start') : '2026-10-19', end_date: p.get('period') === 'custom' ? p.get('end') : '2026-10-25' }, generated_at: stamp, codex_sources: [{ id: 'local', label: 'Local', codex_home: '/demo' }], remotes, current_device_short_code: 'this-device' });
    if (path === '/api/v2/projects') return json({ ...listing(['web-app', 'data-tools', 'docs-site'].filter(key => rows.some(r => r.project === key)).map(key => ({ key, label: key, ...summary(rows.filter(r => r.project === key)), environments: ['Local'], updated_at: stamp }))), current_device: localDeviceInfo });
    if (path === '/api/v2/sessions') return json({ ...listing([...rows].sort((a,b) => p.get('sort') === 'title' ? a.title.localeCompare(b.title) : p.get('sort') === 'tokens' ? b.total_token_usage.total_tokens - a.total_token_usage.total_tokens : (b.estimated_cost_usd || 0) - (a.estimated_cost_usd || 0))), current_device: localDeviceInfo });
    if (path === '/api/v2/project-tasks') return json(page(rows.filter(r => !r.is_subagent && r.project === p.get('project')).map(root => { const group = rows.filter(r => r.uid === root.uid || r.parent_thread_id === root.uid); return { identity: root.uid, sessions: group, summary: summary(group) }; })));
    if (path === '/api/v2/session' && p.get('check_filters') === 'true' && !rows.some(row => row.uid === p.get('id')) && p.get('id') !== 'unknown')
      return route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'session excluded by filters', code: 'session_filtered_out' }) });
    const row = sessions.find(r => r.uid === p.get('id')) || unknown;
    const timeline = Array.from({ length: 135 }, (_, i) => ({ timestamp: new Date(Date.parse(stamp) + i * 1800000 / 134).toISOString(), model: row.model, total_token_usage: usage(Math.round(row.total_token_usage.total_tokens * i / 134)) }));
    if (path === '/api/v2/session') return json({ ...row, revision, has_subagents: sessions.some(child => child.is_subagent && child.environment_id === row.environment_id && child.root_session_id === row.session_id) });
    if (path === '/api/v2/chart') return json({ id: row.uid, revision, timeline: row.uid === 'unknown' ? [] : timeline });
    if (path === '/api/v2/events') {
      const all = row.uid === 'unknown' ? [] : [...timeline].reverse();
      const offset = Number(p.get('cursor') || p.get('offset') || 0);
      return json({ id: row.uid, revision, events: all.slice(offset, offset + 100), offset, total: all.length, limit: 100, next_cursor: offset + 100 < all.length ? String(offset + 100) : null, previous_cursor: offset ? '0' : null });
    }
    if (path === '/api/daily-usage') return json({ generated_at: stamp, daily_usage: [['2026-01-01',108780000],['2026-10-01',6780000],['2026-10-19',2140000],['2026-10-20',3460000],['2026-10-21',4210000],['2026-10-22',3030000]].map(([date, tokens]) => ({ date, usage: usage(tokens) })), daily_usage_complete: true });
    if (request.method() === 'POST') {
      writes++;
      const body = request.postDataJSON();
      if (path.endsWith('/import')) {
        if (!body.label) return json({ needs_label: true, suggested_label: 'Test laptop' });
        remotes = [{ device_short_code: 'test-remote', label: body.label, session_count: 5, usage: usage(), exported_at: stamp }];
      } else if (path.endsWith('/rename')) remotes[0].label = body.label;
      else if (path.endsWith('/delete')) remotes = [];
      else throw new Error('Unexpected mocked write: ' + path);
      return json({ ok: true });
    }
    return route.fulfill({ status: 404, body: 'Unexpected fixture request: ' + path });
  });
  return { writes: () => writes };
}
