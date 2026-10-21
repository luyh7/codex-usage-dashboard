<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { HttpDashboard, DataError, cancelled } from './lib/data/http';
  import type {
    Query,
    Listing,
    Project,
    Session,
    DetailBundle,
    Daily,
    IndexStatus,
  } from './lib/data/types';
  import {
    words,
    number,
    money,
    models,
    fromUrl,
    saveUrl,
    stored,
    save,
    type Language,
  } from './lib/domain/ui';
  import Metrics from './components/Metrics.svelte';
  import Pager from './components/Pager.svelte';
  import ProjectBlock from './components/ProjectBlock.svelte';
  import Calendar from './components/Calendar.svelte';
  import DetailPanel from './components/DetailPanel.svelte';
  import RemoteManager from './components/RemoteManager.svelte';
  const data = new HttpDashboard();
  const initial = fromUrl();
  let query = $state.raw<Query>(initial.query);
  let mode = $state<'projects' | 'sessions'>(initial.mode);
  let selected = $state(initial.selected);
  let displayedMode = $state<'projects' | 'sessions'>(initial.mode);
  let lang = $state<Language>(
    stored('language', navigator.language.startsWith('zh') ? 'zh' : 'en'),
  );
  let theme = $state(stored('theme', 'system'));
  let density = $state(stored('density', 'comfortable'));
  let t = $derived(words(lang));
  let listing = $state.raw<Listing<Project> | Listing<Session> | null>(null);
  let bundle = $state.raw<DetailBundle | null>(null);
  let daily = $state.raw<Daily | null>(null);
  let status = $state.raw<IndexStatus>({ phase: 'loading', revision: 0 });
  let loading = $state(false),
    detailLoading = $state(false),
    dailyLoading = $state(false),
    error = $state(''),
    detailError = $state(''),
    calendar = $state(false),
    remoteOpen = $state(false),
    filters = $state(false),
    newData = $state(false);
  let expanded = $state.raw(new Set<string>()),
    agents = $state.raw(new Set<string>());
  let expansionScope = '';
  let cursor = '';
  let listSequence = 0,
    detailSequence = 0;
  let lastStatusRevision = 0;
  let alive = true;
  let revision = $derived(listing?.page.revision || '');
  let summary = $derived(listing?.page.summary || null);
  let filterChips = $derived(
    [
      ['search', query.search],
      ['environment', query.environment],
      ['source', query.source],
      ['model', query.model],
      ['agents', query.agents],
    ].filter(([, value]) => value),
  );
  $effect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';
    document.documentElement.dataset.density = density;
    save('theme', theme);
    save('density', density);
    save('language', lang);
  });
  function persist(replace = false) {
    saveUrl(query, selected, mode, replace);
  }
  function loadExpansion(device: string) {
    if (expansionScope === device) return;
    expansionScope = device;
    expanded = new Set(stored<string[]>('projects:' + device, []));
    agents = new Set(stored<string[]>('agents:' + device, []));
  }
  function toggleProject(key: string) {
    const next = new Set(expanded);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expanded = next;
    save('projects:' + expansionScope, [...next]);
  }
  function toggleAgent(key: string) {
    const next = new Set(agents);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    agents = next;
    save('agents:' + expansionScope, [...next]);
  }
  async function load(refresh = false, nextCursor = '') {
    const seq = ++listSequence;
    const requestedMode = mode;
    loading = true;
    error = '';
    const currentRevision = nextCursor ? revision : '';
    const conditional = refresh ? listing?.page.query_token : undefined;
    const requestQuery = refresh ? { ...query, offset: listing?.page.offset || 0 } : query;
    try {
      const result =
        mode === 'projects'
          ? await data.projects(requestQuery, nextCursor, currentRevision, conditional)
          : await data.sessions(requestQuery, nextCursor, currentRevision, conditional);
      if (seq !== listSequence || !alive || !result) return;
      const changedRevision = result.page.revision !== revision;
      listing = result;
      displayedMode = requestedMode;
      cursor = nextCursor;
      newData = false;
      loadExpansion(result.current_device_short_code);
      if (
        mode === 'projects' &&
        result.page.total === 1 &&
        !stored('projects:' + expansionScope, null)
      )
        expanded = new Set([(result.page.items[0] as Project).key]);
      if (changedRevision) {
        data.cancelGroups();
        daily = null;
        if (calendar) void loadDaily();
      }
      if (selected && (changedRevision || bundle?.detail.uid !== selected))
        void openDetail(selected, false);
    } catch (e) {
      if (!cancelled(e) && seq === listSequence && alive) {
        error = e instanceof Error ? e.message : String(e);
        if (e instanceof DataError && e.stale) {
          cursor = '';
          void load();
        }
      }
    } finally {
      if (seq === listSequence && alive) loading = false;
    }
  }
  let detailTrigger: HTMLElement | null = null;
  async function openDetail(id: string, history = true) {
    if (history)
      detailTrigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    selected = id;
    if (history) persist();
    const seq = ++detailSequence;
    data.cancelDetails();
    if (bundle?.detail.uid !== id) bundle = null;
    detailError = '';
    detailLoading = true;
    try {
      const result = await data.detail(id, revision);
      if (seq === detailSequence && alive && selected === id) bundle = result;
    } catch (e) {
      if (!cancelled(e) && seq === detailSequence && alive) {
        detailError = e instanceof DataError && e.status === 404 ? t.detailsMissing : String(e);
        if (e instanceof DataError && e.stale) void load();
      }
    } finally {
      if (seq === detailSequence && alive) detailLoading = false;
    }
  }
  async function closeDetail() {
    detailSequence++;
    data.cancelDetails();
    selected = '';
    bundle = null;
    persist();
    await tick();
    if (detailTrigger?.isConnected) detailTrigger.focus();
  }
  let lastSearch = 0;
  function change<K extends keyof Query>(key: K, value: Query[K]) {
    query = { ...query, [key]: value };
    data.cancelGroups();
    data.cancelDetails();
    cursor = '';
    let replace = false;
    if (key === 'search') {
      replace = Date.now() - lastSearch < 1000;
      lastSearch = Date.now();
    }
    persist(replace);
    void load();
  }
  function switchMode(value: 'projects' | 'sessions') {
    mode = value;
    cursor = '';
    persist();
    void load();
  }
  function childError(e: unknown) {
    if (e instanceof DataError && e.stale) {
      void load();
      return;
    }
    error = e instanceof Error ? e.message : String(e);
  }
  async function loadDaily() {
    dailyLoading = true;
    try {
      daily = await data.daily(query.timezone);
    } catch (e) {
      if (!cancelled(e)) childError(e);
    } finally {
      dailyLoading = false;
    }
  }
  function toggleCalendar() {
    calendar = !calendar;
    if (calendar && !daily) void loadDaily();
  }
  function applyRange(start: string, end: string) {
    query = { ...query, period: 'custom', start, end };
    calendar = false;
    cursor = '';
    persist();
    void load();
  }
  function back() {
    const next = fromUrl();
    query = next.query;
    selected = next.selected;
    mode = next.mode;
    cursor = '';
    data.cancelGroups();
    data.cancelDetails();
    void load();
  }
  onMount(() => {
    persist(true);
    void load();
    const unsubscribe = data.subscribe(
      (value) => {
        status = value;
        if (value.phase === 'ready' && value.revision !== lastStatusRevision) {
          lastStatusRevision = value.revision;
          void load(!!listing);
        }
      },
      () => void load(true),
    );
    const poll = setInterval(() => {
      if (!document.hidden) void load(true);
    }, 15000);
    window.addEventListener('popstate', back);
    return () => {
      alive = false;
      clearInterval(poll);
      unsubscribe();
      data.close();
      window.removeEventListener('popstate', back);
    };
  });
</script>

<svelte:head><title>{t.title} · Codex Usage Dashboard</title></svelte:head>
<div class="app-shell">
  <header class="topbar">
    <a class="brand" href="/" aria-label={t.title}
      ><span class="brand-icon">C<span>·</span></span><span
        >{t.title}<small>{t.subtitle}</small></span
      ></a
    >
    <div class="top-actions">
      <button onclick={() => (remoteOpen = true)}>{t.remote}</button><a
        class="button"
        href={data.exportUrl()}>{t.export}</a
      >
      <details class="preferences">
        <summary aria-label={t.theme}>◐</summary>
        <div>
          <label
            >{t.language}<select aria-label={t.language} bind:value={lang}
              ><option value="zh">中文</option><option value="en">English</option></select
            ></label
          ><label
            >{t.theme}<select aria-label={t.theme} bind:value={theme}
              ><option value="system">{t.system}</option><option value="light">{t.light}</option
              ><option value="dark">{t.dark}</option></select
            ></label
          ><label
            >{t.density}<select aria-label={t.density} bind:value={density}
              ><option value="comfortable">{t.comfortable}</option><option value="compact"
                >{t.compact}</option
              ></select
            ></label
          ><label
            >{t.timezone}<input
              value={query.timezone}
              onchange={(e) => change('timezone', e.currentTarget.value)}
            /></label
          >
        </div>
      </details>
    </div>
  </header>
  <main>
    <div class="section-heading">
      <h1>{t.title}</h1>
      <span class="index-status" class:working={status.phase !== 'ready'} role="status"
        ><span class="status-dot"></span>{status.phase === 'ready'
          ? t.ready
          : status.phase === 'error'
            ? t.indexError
            : t.indexing}{#if status.total}
          {number(status.completed, lang)} / {number(
            status.total,
            lang,
          )}{/if}{#if listing && status.phase === 'ready'}
          · {new Date(listing.generated_at).toLocaleTimeString(
            lang === 'zh' ? 'zh-CN' : 'en-US',
          )}{/if}</span
      >
    </div>
    {#if status.error}<p class="error" role="alert">{status.error}</p>{/if}
    <Metrics {summary} {lang} />
    <section class="toolbar" aria-label={t.filters}>
      <label class="search"
        ><span aria-hidden="true">⌕</span><input
          type="search"
          aria-label={t.search}
          placeholder={t.search}
          value={query.search}
          oninput={(e) => change('search', e.currentTarget.value)}
        /></label
      ><label class="period-select"
        ><span class="sr-only">{t.calendar}</span><select
          aria-label={t.calendar}
          value={query.period}
          onchange={(e) => change('period', e.currentTarget.value)}
          >{#each [['today', t.today], ['7d', t.days7], ['30d', t.days30], ['week', t.week], ['month', t.month], ['all', t.all], ['custom', t.custom]] as [value, label]}<option
              {value}>{label}</option
            >{/each}</select
        ></label
      ><button aria-expanded={calendar} onclick={toggleCalendar}
        >{query.period === 'custom' ? query.start + ' — ' + query.end : t.calendar}</button
      ><button aria-expanded={filters} onclick={() => (filters = !filters)}
        >{t.filters}{filterChips.length ? ' · ' + filterChips.length : ''}</button
      >
    </section>
    {#if calendar}<Calendar
        {daily}
        loading={dailyLoading || !daily}
        start={query.start}
        end={query.end}
        {lang}
        apply={applyRange}
      />{/if}
    {#if filters}<section class="filter-panel">
        <label
          >{t.environment}<select
            aria-label={t.environment}
            value={query.environment}
            onchange={(e) => change('environment', e.currentTarget.value)}
            ><option value="">{t.allEnvironments}</option
            >{#each listing?.codex_sources || [] as source (source.id)}<option value={source.id}
                >{source.label}</option
              >{/each}</select
          ></label
        ><label
          >{t.source}<select
            aria-label={t.source}
            value={query.source}
            onchange={(e) => change('source', e.currentTarget.value)}
            ><option value="">{t.all}</option><option value="active">{t.active}</option><option
              value="archived">{t.archived}</option
            ></select
          ></label
        ><label
          >{t.model}<select
            aria-label={t.model}
            value={query.model}
            onchange={(e) => change('model', e.currentTarget.value)}
            ><option value="">{t.allModels}</option
            >{#each listing?.facets?.by_model || summary?.by_model || [] as row (row.model)}<option
                value={row.model}>{row.model}</option
              >{/each}</select
          ></label
        ><label
          >{t.agents}<select
            aria-label={t.agents}
            value={query.agents}
            onchange={(e) => change('agents', e.currentTarget.value)}
            ><option value="">{t.all}</option><option value="main">{t.main}</option><option
              value="agents">{t.onlyAgents}</option
            ></select
          ></label
        >
      </section>{/if}
    {#if filterChips.length}<div class="chips">
        {#each filterChips as [key, value]}<button onclick={() => change(key as keyof Query, '')}
            >{value} <span aria-hidden="true">×</span><span class="sr-only">
              {t.clear}</span
            ></button
          >{/each}<button
          class="text-button"
          onclick={() => {
            query = { ...query, search: '', environment: '', source: '', model: '', agents: '' };
            persist();
            void load();
          }}>{t.clear}</button
        >
      </div>{/if}
    <div class="workspace" class:with-detail={!!selected}>
      <section class="browser-pane" aria-busy={loading}>
        <div class="list-heading">
          <div class="view-tabs" role="group" aria-label={t.show}>
            <button
              class:chosen={mode === 'projects'}
              aria-pressed={mode === 'projects'}
              onclick={() => switchMode('projects')}>{t.projects}</button
            ><button
              class:chosen={mode === 'sessions'}
              aria-pressed={mode === 'sessions'}
              onclick={() => switchMode('sessions')}>{t.conversations}</button
            >
          </div>
          <div class="sort">
            <label
              ><span class="sr-only">{t.sort}</span><select
                aria-label={t.sort}
                value={query.sort}
                onchange={(e) => change('sort', e.currentTarget.value)}
                ><option value="tokens">Tokens</option><option value="cost">{t.cost}</option><option
                  value="updated">{t.updated}</option
                ><option value="title">{t.name}</option></select
              ></label
            ><button
              aria-label={query.ascending ? t.ascending : t.descending}
              onclick={() => change('ascending', !query.ascending)}
              >{query.ascending ? '↑' : '↓'}</button
            >
          </div>
        </div>
        {#if error}<p class="error" role="alert">
            {error} <button onclick={() => load()}>{t.retry}</button>
          </p>{/if}{#if newData}<p class="notice">
            <button onclick={() => load()}>{t.ready} · {t.retry}</button>
          </p>{/if}{#if loading && !listing}<p class="empty" role="status">
            {t.loading}
          </p>{/if}{#if listing && !listing.page.total}<p class="empty">{t.empty}</p>{/if}
        {#if listing && displayedMode === 'projects'}{#each listing.page.items as Project[] as project (project.key)}<ProjectBlock
              {project}
              {query}
              {revision}
              {data}
              {selected}
              {lang}
              open={expanded.has(project.key)}
              toggle={() => toggleProject(project.key)}
              select={(row) => openDetail(row.uid)}
              agentExpanded={agents}
              {toggleAgent}
              onerror={childError}
            />{/each}
        {:else if listing}<div class="table-scroll">
            <table class="sessions-table">
              <thead
                ><tr
                  ><th>{t.conversations}</th><th class="numeric">Tokens</th><th class="numeric"
                    >{t.cost}</th
                  ><th>{t.projects}</th></tr
                ></thead
              ><tbody
                >{#each listing.page.items as Session[] as row (row.uid)}<tr
                    class:selected={selected === row.uid}
                    data-uid={row.uid}
                    ><td
                      ><button class="session-title" onclick={() => openDetail(row.uid)}
                        >{row.title || row.session_id}<small
                          >{row.environment} · {models(row).join(', ')}{row.is_subagent
                            ? ' · agent'
                            : ''}</small
                        ></button
                      ></td
                    ><td class="numeric">{number(row.total_token_usage?.total_tokens, lang)}</td><td
                      class="numeric">{money(row.estimated_cost_usd, lang)}</td
                    ><td>{row.project}</td></tr
                  >{/each}</tbody
              >
            </table>
          </div>{/if}
        {#if listing}<Pager
            offset={listing.page.offset}
            total={listing.page.total}
            limit={listing.page.limit}
            next={listing.page.next_cursor}
            previous={listing.page.previous_cursor}
            disabled={loading}
            change={(next) => load(false, next)}
            {lang}
          />{/if}
      </section>
      {#if selected}<DetailPanel
          {bundle}
          loading={detailLoading}
          error={detailError}
          {lang}
          zone={query.timezone}
          {data}
          close={closeDetail}
          retry={() => openDetail(selected, false)}
          onerror={childError}
        />{/if}
    </div>
    <footer>{t.localOnly} · <a href="/legacy">Legacy UI</a></footer>
  </main>
</div>
{#if remoteOpen}<RemoteManager
    {data}
    remotes={listing?.remotes || []}
    {lang}
    close={() => (remoteOpen = false)}
    changed={() => load()}
  />{/if}
