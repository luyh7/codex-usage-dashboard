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
  import Icon from './components/Icon.svelte';
  import { compactNumber, displayMoney } from './lib/domain/presentation';
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
    document.documentElement.dataset.theme = 'clear-mint';
    document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';
    document.documentElement.dataset.density = density;
    save('theme', 'clear-mint');
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
    if (key === 'timezone') daily = null;
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
  let calendarTrigger = $state<HTMLButtonElement>();
  async function closeCalendar(restoreFocus = true) {
    calendar = false;
    await tick();
    if (restoreFocus) calendarTrigger?.focus({ preventScroll: true });
  }
  function applyRange(start: string, end: string, period = 'custom') {
    query = {
      ...query,
      period,
      start: period === 'custom' ? start : '',
      end: period === 'custom' ? end : '',
    };
    void closeCalendar();
    cursor = '';
    persist();
    void load();
  }
  function back() {
    const next = fromUrl();
    calendar = false;
    query = next.query;
    selected = next.selected;
    mode = next.mode;
    cursor = '';
    data.cancelGroups();
    data.cancelDetails();
    void load();
  }
  let periods = $derived([
    ['today', t.today],
    ['7d', t.days7],
    ['30d', t.days30],
    ['week', t.week],
    ['month', t.month],
    ['all', t.all],
    ['custom', t.custom],
  ]);
  let rangeLabel = $derived.by(() => {
    const start = listing?.period.start_date,
      end = listing?.period.end_date;
    if (!start || !end) return query.period === 'all' ? t.all : t.calendar;
    if (lang === 'zh')
      return `${start.slice(0, 4)}年${Number(start.slice(5, 7))}月${Number(start.slice(8))}日 – ${end.slice(0, 4) === start.slice(0, 4) ? '' : end.slice(0, 4) + '年'}${Number(end.slice(5, 7))}月${Number(end.slice(8))}日`;
    return `${start.replaceAll('-', '/')} – ${end.replaceAll('-', '/')}`;
  });
  function choosePeriod(value: string) {
    if (value === 'custom') {
      calendar = true;
      if (!daily) void loadDaily();
    } else {
      calendar = false;
      change('period', value);
    }
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

<svelte:head
  ><title>{mode === 'projects' ? t.projectUsage : t.overview} · Codex Usage Dashboard</title
  ></svelte:head
>
<div class="app-shell" data-view={mode}>
  <aside class="sidebar">
    <a class="brand" href="/svelte" aria-label="Codex Usage Dashboard">
      <span
        ><span class="brand-word">Codex</span><br /><span class="brand-word">Usage</span><br /><span
          class="brand-word">Dashboard</span
        ></span
      >
    </a>
    <nav class="main-nav" aria-label={t.navigation}>
      <button
        class:chosen={mode === 'sessions'}
        aria-current={mode === 'sessions' ? 'page' : undefined}
        onclick={() => switchMode('sessions')}
        ><Icon name="overview" size={24} /><span>{t.overviewNav}</span></button
      >
      <button
        class:chosen={mode === 'projects'}
        aria-current={mode === 'projects' ? 'page' : undefined}
        onclick={() => switchMode('projects')}
        ><Icon name="folder" size={24} /><span>{t.projects}</span></button
      >
      <button aria-haspopup="dialog" onclick={() => (remoteOpen = true)}
        ><Icon name="devices" size={24} /><span>{t.devices}</span>{#if listing?.remotes.length}<span
            class="nav-count">{listing.remotes.length}</span
          >{/if}</button
      >
    </nav>
    <div class="sidebar-bottom">
      <div class="local-status" title={t.localHint}>
        <span class="status-dot"></span><span>{t.localRunning}</span>
      </div>
      <details class="preferences">
        <summary aria-label={t.settings}
          ><Icon name="settings" size={19} /><span>{t.settings}</span></summary
        >
        <div>
          <h3>{t.settings}</h3>
          <label
            >{t.language}<select aria-label={t.language} bind:value={lang}
              ><option value="zh">中文</option><option value="en">English</option></select
            ></label
          >
          <label
            >{t.density}<select aria-label={t.density} bind:value={density}
              ><option value="comfortable">{t.comfortable}</option><option value="compact"
                >{t.compact}</option
              ></select
            ></label
          >
          <label
            >{t.timezone}<input
              value={query.timezone}
              onchange={(e) => change('timezone', e.currentTarget.value)}
            /></label
          >
        </div>
      </details>
    </div>
  </aside>
  <main>
    <header class="page-header" class:projects={mode === 'projects'}>
      <div class="page-title">
        <h1>{mode === 'projects' ? t.projectUsage : t.overview}</h1>
        {#if mode === 'projects'}<p>{t.projectSubtitle}</p>{/if}
      </div>
      <div class="date-controls">
        <div class="period-tabs" role="group" aria-label={t.period}>
          {#each periods.filter( ([value]) => ['today', '7d', 'week', 'month'].includes(value) ) as [value, label]}
            <button
              class:chosen={query.period === value}
              aria-pressed={query.period === value}
              onclick={() => choosePeriod(value)}>{label}</button
            >
          {/each}
        </div>
        <select
          class="mobile-period"
          aria-label={t.period}
          value={query.period}
          onchange={(e) => choosePeriod(e.currentTarget.value)}
          >{#each periods as [value, label]}<option {value}>{label}</option>{/each}</select
        >
        <div class="calendar-anchor">
          <button
            bind:this={calendarTrigger}
            class="calendar-trigger"
            class:expanded={calendar}
            aria-label={t.calendar}
            aria-expanded={calendar}
            aria-haspopup="dialog"
            aria-controls="date-picker"
            onclick={toggleCalendar}
          >
            <Icon name="calendar" size={19} /><span>{rangeLabel}</span><Icon
              name="down"
              size={15}
            />
          </button>
          {#if calendar}<Calendar
              {daily}
              loading={dailyLoading || !daily}
              start={listing?.period.start_date || query.start}
              end={listing?.period.end_date || query.end}
              period={query.period}
              zone={query.timezone}
              {lang}
              apply={applyRange}
              close={closeCalendar}
              trigger={calendarTrigger}
            />{/if}
        </div>
      </div>
    </header>
    {#if status.error}<p class="error" role="alert">{status.error}</p>{/if}
    <Metrics {summary} {lang} />
    <div class="workspace" class:with-detail={!!selected}>
      <section id="usage-browser" class="browser-pane" aria-busy={loading}>
        <header class="list-heading">
          <div>
            <h2>{mode === 'projects' ? t.allProjects : t.conversationUsage}</h2>
            <span class="list-count"
              >{number(mode === 'projects' ? listing?.page.total : summary?.session_count, lang)}
              {mode === 'projects' ? t.projectCount : t.sessionCount}</span
            >
          </div>
          <span
            class="index-status"
            class:working={status.phase !== 'ready'}
            title={listing
              ? new Date(listing.generated_at).toLocaleString(lang === 'zh' ? 'zh-CN' : 'en-US')
              : ''}
            role="status"
            ><span class="status-dot"></span>{status.phase === 'ready'
              ? t.ready
              : status.phase === 'error'
                ? t.indexError
                : t.indexing}{#if status.total}
              {number(status.completed, lang)}/{number(status.total, lang)}{/if}</span
          >
        </header>
        <section class="toolbar" aria-label={t.filters}>
          <label class="search"
            ><Icon name="search" size={18} /><input
              type="search"
              aria-label={t.search}
              placeholder={mode === 'projects' ? t.projectSearch : t.search}
              value={query.search}
              oninput={(e) => change('search', e.currentTarget.value)}
            /></label
          >
          <button
            class:chosen={filters}
            aria-label={t.filters + (filterChips.length ? ' · ' + filterChips.length : '')}
            aria-expanded={filters}
            onclick={() => (filters = !filters)}
            ><Icon name="filter" size={16} /><span
              >{t.filters}{filterChips.length ? ' · ' + filterChips.length : ''}</span
            ><Icon name="down" size={13} /></button
          >
          <div class="sort">
            <label
              ><span class="sr-only">{t.sort}</span><select
                aria-label={t.sort}
                value={query.sort}
                onchange={(e) => change('sort', e.currentTarget.value)}
                ><option value="cost">{query.ascending ? t.costAscending : t.costDescending}</option
                ><option value="tokens">{t.tokens}</option><option value="updated"
                  >{t.updated}</option
                ><option value="title">{t.name}</option></select
              ></label
            ><button
              aria-label={query.ascending ? t.ascending : t.descending}
              title={query.ascending ? t.ascending : t.descending}
              onclick={() => change('ascending', !query.ascending)}
              ><span class:ascending={query.ascending}><Icon name="arrow" size={16} /></span
              ></button
            >
          </div>
        </section>
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
            {#each filterChips as [key, value]}<button
                onclick={() => change(key as keyof Query, '')}
                >{value} <span aria-hidden="true">×</span><span class="sr-only">
                  {t.clear}</span
                ></button
              >{/each}<button
              class="text-button"
              onclick={() => {
                query = {
                  ...query,
                  search: '',
                  environment: '',
                  source: '',
                  model: '',
                  agents: '',
                };
                persist();
                void load();
              }}>{t.clear}</button
            >
          </div>{/if}

        {#if error}<p class="error" role="alert">
            {error} <button onclick={() => load()}>{t.retry}</button>
          </p>{/if}
        {#if newData}<p class="notice">
            <button onclick={() => load()}>{t.ready} · {t.retry}</button>
          </p>{/if}
        {#if loading && !listing}<div class="list-skeleton" role="status" aria-label={t.loading}>
            {#each [0, 1, 2, 3] as row}<div><span></span><span></span></div>{/each}
          </div>{/if}
        {#if listing && !listing.page.total}<div class="empty">
            <span class="empty-icon"><Icon name="search" size={28} /></span>
            <h3>{t.empty}</h3>
            <p>{t.emptyHint}</p>
            {#if filterChips.length}<button
                onclick={() => {
                  query = {
                    ...query,
                    search: '',
                    environment: '',
                    source: '',
                    model: '',
                    agents: '',
                  };
                  persist();
                  void load();
                }}>{t.clear}</button
              >{/if}
          </div>{/if}
        {#if listing && displayedMode === 'projects'}
          {#if listing.page.total}<div class="project-columns">
              <span>{t.name}</span><span class="numeric">{t.usageColumn}</span><span class="numeric"
                >{t.cost}<Icon name="arrow" size={15} /></span
              >
            </div>{/if}
          {#each listing.page.items as Project[] as project (project.key)}
            <ProjectBlock
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
            />
          {/each}
        {:else if listing && listing.page.total}
          <div class="table-scroll">
            <table class="sessions-table">
              <thead
                ><tr
                  ><th>{t.conversations}</th><th class="project-column">{t.projectLabel}</th><th
                    class="model-column">{t.model}</th
                  ><th class="numeric">{t.usageColumn}</th><th class="numeric">{t.cost}</th></tr
                ></thead
              >
              <tbody
                >{#each listing.page.items as Session[] as row (row.uid)}
                  <tr class:selected={selected === row.uid} data-uid={row.uid}
                    ><td
                      ><div class="title-cell">
                        <span class="session-icon"><Icon name="message" size={18} /></span><button
                          class="session-title"
                          onclick={() => openDetail(row.uid)}
                          title={row.title || row.session_id}
                          ><span class="title-text">{row.title || row.session_id}</span><small
                            class="mobile-session-meta"
                            ><span class="project-tag">{row.project}</span><span
                              >{models(row).join(', ')}</span
                            >{#if row.is_subagent}<span>{t.onlyAgents}</span>{/if}</small
                          ></button
                        >
                      </div></td
                    >
                    <td class="project-column"
                      ><span class="project-tag" title={row.project}>{row.project}</span></td
                    >
                    <td class="model-column"
                      ><span class="model-tag" title={models(row).join(', ')}
                        >{models(row).join(', ')}</span
                      ></td
                    >
                    <td class="numeric" title={number(row.total_token_usage?.total_tokens, lang)}
                      >{compactNumber(row.total_token_usage?.total_tokens)}</td
                    >
                    <td class="numeric cost-cell" title={money(row.estimated_cost_usd, lang)}
                      >{row.estimated_cost_usd == null
                        ? t.unknown
                        : displayMoney(row.estimated_cost_usd)}</td
                    ></tr
                  >
                {/each}</tbody
              >
            </table>
          </div>
        {/if}
        {#if listing && (listing.page.next_cursor || listing.page.previous_cursor)}<Pager
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
          projectView={mode === 'projects'}
          {bundle}
          loading={detailLoading}
          error={detailError}
          {lang}
          zone={query.timezone}
          {data}
          close={closeDetail}
          retry={() => openDetail(selected, false)}
          onerror={childError}
        />
      {:else}<aside class="detail-placeholder">
          <span class="placeholder-art"><Icon name="message" size={36} /></span>
          <h2>{t.selectSession}</h2>
          <p>{t.selectHint}</p>
          <span class="placeholder-tip"><Icon name="chevron" size={14} />{t.selectTip}</span>
        </aside>{/if}
    </div>
    <footer>
      <span><Icon name="shield" size={14} />{t.localOnly} · {t.estimatedHint}</span><a
        href="/legacy">{t.classic}</a
      >
    </footer>
  </main>
</div>
{#if remoteOpen}<RemoteManager
    {data}
    remotes={listing?.remotes || []}
    {lang}
    close={() => (remoteOpen = false)}
    changed={() => load()}
  />{/if}
