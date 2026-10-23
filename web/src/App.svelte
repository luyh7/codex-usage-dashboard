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
    dateTime,
    fromUrl,
    saveUrl,
    stored,
    save,
    deviceLanguage,
    type Language,
  } from './lib/domain/ui';
  import Icon from './components/Icon.svelte';
  import brandIcon from './assets/cud-soft-icon.svg';
  import brandTitle from './assets/cud-soft-title-light.svg';
  import { tooltips } from './lib/domain/tooltip';
  import { usageSpeedHint } from './lib/domain/speed';
  import AnimatedValue from './components/AnimatedValue.svelte';
  import { compactNumber, displayMoney, activityTime } from './lib/domain/presentation';
  import RelativeTime from './components/RelativeTime.svelte';
  import SortMenu from './components/SortMenu.svelte';
  import GlobalFilters from './components/GlobalFilters.svelte';
  import Dropdown from './components/Dropdown.svelte';
  import ArchiveTag from './components/ArchiveTag.svelte';
  import SessionMetadata from './components/SessionMetadata.svelte';
  import CacheHit from './components/CacheHit.svelte';
  import Metrics from './components/Metrics.svelte';
  import Pager from './components/Pager.svelte';
  import ProjectBlock from './components/ProjectBlock.svelte';
  import Calendar from './components/Calendar.svelte';
  import DetailPanel from './components/DetailPanel.svelte';
  import SettingsPage from './components/SettingsPage.svelte';
  import VirtualProjects from './components/VirtualProjects.svelte';
  import { ProjectCache } from './lib/data/projectCache';
  const data = new HttpDashboard();
  const projectCache = new ProjectCache();
  const initial = fromUrl();
  let page = $state(initial.page);
  let query = $state.raw<Query>(initial.query);
  let displayedQuery = $state.raw<Query>(initial.query);
  let mode = $state<'projects' | 'sessions'>(initial.mode);
  let selected = $state(initial.selected);
  let displayedMode = $state<'projects' | 'sessions'>(initial.mode);
  let lang = $state<Language>(
    stored('language', deviceLanguage()),
  );
  let t = $derived(words(lang));
  let listing = $state.raw<Listing<Project> | Listing<Session> | null>(null);
  let bundle = $state.raw<DetailBundle | null>(null);
  let daily = $state.raw<Daily | null>(null);
  let dailyTimezone = $state('');
  let status = $state.raw<IndexStatus>({ phase: 'loading', revision: 0 });
  // A published listing stays usable while the server updates its index in the background.
  let indexPhase = $derived(listing && status.phase !== 'error' ? 'ready' : status.phase);
  let loading = $state(false),
    detailLoading = $state(false),
    error = $state(''),
    detailError = $state(''),
    detailRetryable = $state(true),
    calendar = $state(false),
    newData = $state(false);
  let slowLoading = $state(false);
  let moreLoading = $state(false);
  let moreError = $state('');
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let progressTimer: ReturnType<typeof setTimeout> | undefined;
  const queryKey = (value: Query) => JSON.stringify({ ...value, offset: 0 });
  let pendingResults = $derived(!!listing && (mode !== displayedMode || queryKey(query) !== queryKey(displayedQuery)));
  let expanded = $state.raw(new Set<string>()),
    agents = $state.raw(new Set<string>());
  let expansionScope = '';
  let cursor = '';
  let listSequence = 0,
    detailSequence = 0,
    dailySequence = 0;
  let dailyRevision = '';
  let lastStatusRevision = 0;
  let alive = true;
  let revision = $derived(listing?.page.revision || '');
  let calendarDaily = $derived(dailyTimezone === query.timezone ? daily : null);
  let updatedTime = $derived(
    listing
      ? new Intl.DateTimeFormat(lang === 'zh' ? 'zh-CN' : 'en-US', {
          timeZone: query.timezone,
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
          hourCycle: 'h23',
        }).format(new Date(listing.generated_at))
      : '',
  );
  let summary = $derived(listing?.page.summary || null);
  let showEnvironment = $derived((listing?.codex_sources.length ?? 0) > 1);
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
    save('theme', 'clear-mint');
    save('language', lang);
  });
  function persist(replace = false) {
    saveUrl(query, selected, mode, replace, page);
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
    if (refresh && (loading || moreLoading || searchTimer)) return;
    clearTimeout(searchTimer);
    searchTimer = undefined;
    clearTimeout(progressTimer);
    if (refresh && pendingResults) refresh = false;
    const seq = ++listSequence;
    const requestedMode = mode;
    const requestedQuery = { ...query };
    const previousCount = refresh && displayedMode === 'projects' ? listing?.page.items.length || 30 : 30;
    data.cancelListing();
    moreLoading = false;
    moreError = '';
    loading = true;
    progressTimer = setTimeout(() => { if (seq === listSequence) slowLoading = true; }, 150);
    error = '';
    const currentRevision = nextCursor ? revision : '';
    const conditional = refresh ? listing?.page.query_token : undefined;
    const requestQuery = refresh ? { ...requestedQuery, offset: listing?.page.offset || 0 } : requestedQuery;
    try {
      let result =
        mode === 'projects'
          ? await data.projects(requestQuery, nextCursor, currentRevision, conditional, Math.min(100, previousCount))
          : await data.sessions(requestQuery, nextCursor, currentRevision, conditional);
      if (seq !== listSequence || !alive) return;
      if (!result) {
        syncDetail();
        return;
      }
      if (requestedMode === 'projects') {
        const projects = result as Listing<Project>;
        const items = [...projects.page.items];
        let next = projects.page.next_cursor;
        while (next && items.length < previousCount) {
          const more = await data.projects(requestQuery, next, projects.page.revision, '', 100);
          if (seq !== listSequence || !alive || !more) return;
          items.push(...more.page.items);
          next = more.page.next_cursor;
        }
        result = { ...projects, page: { ...projects.page, items, next_cursor: next, previous_cursor: null } };
      }
      const changedRevision = result.page.revision !== revision;
      listing = result;
      displayedMode = requestedMode;
      displayedQuery = requestedQuery;
      cursor = nextCursor;
      newData = false;
      loadExpansion(result.current_device_short_code);
      if (
        mode === 'projects' &&
        !stored('projects:' + expansionScope, null)
      )
        expanded = new Set((result.page.items as Project[]).map((project) => project.key));
      if (changedRevision) {
        data.cancelGroups();
        // Keep the previous daily totals visible until their replacement arrives.
        if (calendar) void loadDaily();
      }
      syncDetail(changedRevision);
    } catch (e) {
      if (!cancelled(e) && seq === listSequence && alive) {
        error = e instanceof Error ? e.message : String(e);
        if (e instanceof DataError && e.stale) {
          cursor = '';
          void load();
        }
      }
    } finally {
      if (seq === listSequence && alive) {
        loading = false;
        slowLoading = false;
        clearTimeout(progressTimer);
      }
    }
  }
  async function loadMoreProjects() {
    if (loading || moreLoading || pendingResults || moreError || displayedMode !== 'projects' || !listing?.page.next_cursor) return;
    const previous = listing as Listing<Project>;
    const seq = listSequence;
    moreLoading = true;
    try {
      const result = await data.projects(displayedQuery, previous.page.next_cursor!, previous.page.revision);
      if (!alive || seq !== listSequence || listing !== previous || !result) return;
      listing = { ...previous, page: { ...previous.page, items: [...previous.page.items, ...result.page.items], next_cursor: result.page.next_cursor } };
      if (!stored('projects:' + expansionScope, null))
        expanded = new Set([...expanded, ...result.page.items.map(project => project.key)]);
    } catch (e) {
      if (!cancelled(e) && seq === listSequence) {
        if (e instanceof DataError && e.stale) {
          moreLoading = false;
          void load(true);
        }
        else moreError = String(e);
      }
    } finally {
      if (seq === listSequence) moreLoading = false;
    }
  }
  let detailTrigger: HTMLElement | null = null;
  let detailScope = '';
  const selectionScope = (value: Query) => JSON.stringify([
    value.period, value.start, value.end, value.timezone, value.search,
    value.environment, value.source, value.model, value.agents,
  ]);
  function syncDetail(changedRevision = false) {
    if (selected && (changedRevision || bundle?.detail.uid !== selected || detailScope !== selectionScope(query))) {
      loading = false;
      void openDetail(selected, false);
    }
  }
  function invalidateDetails() {
    detailSequence++;
    data.cancelDetails();
    detailScope = '';
    detailError = '';
    detailRetryable = true;
    detailLoading = !!selected;
  }
  async function openDetail(id: string, history = true) {
    if (history)
      detailTrigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    selected = id;
    if (history) persist();
    if (loading) {
      if (bundle?.detail.uid !== id) bundle = null;
      invalidateDetails();
      return;
    }
    const seq = ++detailSequence;
    const requestedRevision = revision;
    const requestedQuery = { ...query };
    const requestedScope = selectionScope(requestedQuery);
    detailScope = requestedScope;
    const current = () => seq === detailSequence && alive && selected === id
      && requestedRevision === revision && requestedScope === selectionScope(query);
    data.cancelDetails();
    if (bundle?.detail.uid !== id) bundle = null;
    detailError = '';
    detailRetryable = true;
    detailLoading = true;
    try {
      const result = await data.detail(id, requestedRevision, requestedQuery);
      if (current()) bundle = result;
    } catch (e) {
      if (!cancelled(e) && current()) {
        if (e instanceof DataError && ['session_filtered_out', 'session_not_found'].includes(e.code)) {
          clearDetail(true);
        } else if (e instanceof DataError && e.stale) {
          void load();
        } else {
          bundle = null;
          detailRetryable = !(e instanceof DataError && e.code === 'details_missing');
          detailError = detailRetryable ? String(e) : t.detailsMissing;
        }
      }
    } finally {
      if (current()) detailLoading = false;
    }
  }
  function clearDetail(replace = false) {
    detailSequence++;
    data.cancelDetails();
    selected = '';
    bundle = null;
    detailError = '';
    detailLoading = false;
    detailRetryable = true;
    detailScope = '';
    persist(replace);
  }
  async function closeDetail() {
    clearDetail();
    await tick();
    if (detailTrigger?.isConnected) detailTrigger.focus({ preventScroll: true });
  }
  let lastSearch = 0;
  function change<K extends keyof Query>(key: K, value: Query[K], updates: Partial<Query> = {}) {
    query = { ...query, ...updates, [key]: value };
    data.cancelGroups();
    invalidateDetails();
    cursor = '';
    let replace = false;
    if (key === 'search') {
      replace = Date.now() - lastSearch < 1000;
      lastSearch = Date.now();
    }
    persist(replace);
    if (key === 'search' && value) {
      clearTimeout(searchTimer);
      clearTimeout(progressTimer);
      data.cancelListing();
      listSequence++;
      moreLoading = false;
      loading = true;
      progressTimer = setTimeout(() => slowLoading = true, 150);
      searchTimer = setTimeout(() => { searchTimer = undefined; void load(); }, 180);
    } else void load();
  }
  function switchMode(value: 'projects' | 'sessions') {
    if (mode === value) return;
    data.cancelGroups();
    invalidateDetails();
    mode = value;
    document.querySelector('.browser-scroll')?.scrollTo({ top: 0 });
    cursor = '';
    persist();
    void load();
  }
  function showOverview() {
    calendar = false;
    if (page === 'settings') {
      navigate('overview');
      return;
    }
    if (selected) void closeDetail();
    switchMode('projects');
    const browser = document.querySelector<HTMLElement>('.browser-scroll');
    browser?.scrollTo({ top: 0 });
    browser?.focus({ preventScroll: true });
  }
  async function navigate(next: 'overview' | 'settings') {
    if (page === next) return;
    calendar = false;
    page = next;
    persist();
    await tick();
    document.querySelector<HTMLElement>(next === 'settings' ? '.settings-header h1' : '.overview-page h1')?.focus({ preventScroll: true });
  }
  function childError(e: unknown) {
    if (e instanceof DataError && e.stale) {
      void load();
      return;
    }
    error = e instanceof Error ? e.message : String(e);
  }
  async function loadDaily() {
    const seq = ++dailySequence;
    const timezone = query.timezone;
    const requestedRevision = revision;
    try {
      const result = await data.daily(timezone);
      if (seq !== dailySequence || !alive || timezone !== query.timezone) return;
      daily = result;
      dailyTimezone = timezone;
      dailyRevision = requestedRevision;
    } catch (e) {
      if (!cancelled(e) && seq === dailySequence && alive && timezone === query.timezone)
        childError(e);
    }
  }
  function toggleCalendar() {
    calendar = !calendar;
    if (calendar && (!calendarDaily || dailyRevision !== revision)) void loadDaily();
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
    data.cancelGroups();
    invalidateDetails();
    void closeCalendar();
    cursor = '';
    persist();
    void load();
  }
  function back() {
    const next = fromUrl();
    page = next.page;
    calendar = false;
    query = next.query;
    selected = next.selected;
    mode = next.mode;
    cursor = '';
    data.cancelGroups();
    invalidateDetails();
    if (bundle?.detail.uid !== selected) bundle = null;
    void load();
  }
  let periods = $derived<[string, string][]>([
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
      if (!calendarDaily || dailyRevision !== revision) void loadDaily();
    } else {
      calendar = false;
      change('period', value);
    }
  }
  function clearFilters() {
    query = { ...query, search: '', environment: '', source: '', model: '', agents: '' };
    data.cancelGroups();
    invalidateDetails();
    cursor = '';
    persist();
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
      clearTimeout(searchTimer);
      clearTimeout(progressTimer);
      clearInterval(poll);
      unsubscribe();
      data.close();
      window.removeEventListener('popstate', back);
    };
  });
</script>

<svelte:head>
  <title>Codex Usage Dashboard</title>
  <link rel="icon" type="image/svg+xml" href={brandIcon} />
</svelte:head>
<div class="app-shell" data-view={mode} data-page={page} use:tooltips>
  <aside class="sidebar">
    <div class="brand">
      <img src={brandIcon} width="40" height="40" alt="Codex Usage Dashboard" />
    </div>
    <nav class="main-nav" aria-label={t.navigation}>
      <button
        class:chosen={page === 'overview'}
        aria-current={page === 'overview' ? 'page' : undefined}
        aria-label={t.overviewNav}
        data-tooltip={t.overviewNav}
        onclick={showOverview}
        ><Icon name="overview" size={24} /></button
      >
      <button class:chosen={page === 'settings'} aria-current={page === 'settings' ? 'page' : undefined} aria-label={t.settings} data-tooltip={t.settings} onclick={() => navigate('settings')}
        ><Icon name="settings" size={24} /></button
      >
    </nav>
    <div class="sidebar-bottom">
    </div>
  </aside>
  <main>
    {#if page === 'settings'}
      <SettingsPage {data} remotes={listing?.remotes || []} localDevice={listing?.current_device} {lang} timezone={query.timezone}
        languageChanged={(value) => lang = value}
        timezoneChanged={(value) => { save('timezone', value); change('timezone', value); }}
        changed={() => load()} />
    {/if}
    <div class="overview-page" hidden={page !== 'overview'} inert={page !== 'overview'}>
    <header class="page-header">
      <div class="page-title">
        <h1 tabindex="-1"><img class="brand-title" src={brandTitle} width="118" height="50" alt="Codex Usage Dashboard" /></h1>
      </div>
    <GlobalFilters
      {query}
      {lang}
      sources={listing?.codex_sources || []}
      modelOptions={(listing?.facets?.by_model || summary?.by_model || []).map((row) => row.model)}
      {change}
      clear={clearFilters}
      apply={(filters) => change('environment', filters.environment, filters)}
    >
      {#snippet dateControls()}
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
        <div class="mobile-period"><Dropdown label={t.period} value={query.period} options={periods} change={choosePeriod} /></div>
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
              daily={calendarDaily}
              loading={!calendarDaily}
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
      {/snippet}
    </GlobalFilters>
    </header>
    {#if status.error}<p class="error" role="alert">{status.error}</p>{/if}
    <div class="results" class:pending={pendingResults}>
    <Metrics {summary} {lang} trend={listing?.metric_trend} period={listing?.period ?? null} />
    <div class="workspace" class:with-detail={!!selected}>
      <section id="usage-browser" class="browser-pane" aria-busy={loading}>
        {#if slowLoading}<div class="results-progress" aria-hidden="true"></div>{/if}
        <header class="list-heading">
          <div>
            <h2>{t.conversationUsage}</h2>
            <span class="list-count"
              ><AnimatedValue
                value={(mode === 'projects' ? listing?.page.total : summary?.session_count) ?? null}
                format={(value) => number(Math.round(value), lang)}
              />
              {mode === 'projects' ? t.projectCount : t.sessionCount}</span
            >
          </div>
          <div class="list-actions">
            <SortMenu value={query.sort} ascending={query.ascending} {lang} change={(value) => change('sort', value)} reverse={() => change('ascending', !query.ascending)} />
            <div class="view-switch" role="group" aria-label={t.displayMode}>
              <button class:chosen={mode === 'projects'} aria-pressed={mode === 'projects'} onclick={() => switchMode('projects')}>{t.byProject}</button>
              <button class:chosen={mode === 'sessions'} aria-pressed={mode === 'sessions'} onclick={() => switchMode('sessions')}>{t.byConversation}</button>
            </div>
          <span
            class="index-status"
            class:working={slowLoading || indexPhase !== 'ready'}
            data-tooltip={pendingResults ? t.previousResults : listing ? dateTime(listing.generated_at, query.timezone, lang) : ''}
            role="status"
            ><span class="status-dot"></span>{#if slowLoading}{t.updating}{:else if indexPhase === 'ready'}<span
                >{t.ready}{#if listing}{' '}
                  <time datetime={listing.generated_at}>{updatedTime}</time>{/if}</span
              >{:else}{indexPhase === 'error'
                ? t.indexError
                : t.indexing}{/if}{#if indexPhase !== 'ready' && status.total}
              {number(status.completed, lang)}/{number(status.total, lang)}{/if}</span
          >
          </div>
        </header>
        <!-- svelte-ignore a11y_no_noninteractive_tabindex (This scroll region must support keyboard scrolling independently of the page.) -->
        <div
          class="browser-scroll"
          tabindex="0"
          role="region"
          aria-label={mode === 'projects' ? t.allProjects : t.conversationUsage}
        >
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
              {#if filterChips.length}<button onclick={clearFilters}>{t.clear}</button>{/if}
            </div>{/if}
          {#if listing && displayedMode === 'projects'}
            {#if listing.page.total}<div class="project-columns">
                <span>{t.conversations}</span><span class="numeric" aria-label={t.usageColumn}><span class="usage-column-label">{t.usageColumn}</span><span class="usage-column-short">{t.tokens}</span></span><span
                  class="numeric">{t.cost}{#if query.sort === 'cost'}<Icon name="arrow" size={15} />{/if}</span
                ><span class="numeric cache-column">{t.cacheHit}</span>
              </div>{/if}
            <VirtualProjects projects={listing.page.items as Project[]} scope={queryKey(displayedQuery)}
              active={page === 'overview'} more={!!listing.page.next_cursor} loading={loading || moreLoading || !!moreError}
              loadMore={() => void loadMoreProjects()}>
            {#snippet row(project)}
              <ProjectBlock
                {project}
                {showEnvironment}
                query={displayedQuery}
                zone={query.timezone}
                cache={projectCache}
                active={page === 'overview' && !loading}
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
            {/snippet}
            </VirtualProjects>
            {#if moreLoading}<div class="project-load-state" role="status">{t.loading}</div>{/if}
            {#if moreError}<p class="error" role="alert">{moreError} <button onclick={() => { moreError = ''; void loadMoreProjects(); }}>{t.retry}</button></p>{/if}
          {:else if listing && listing.page.total}
            <div class="table-scroll">
              <table class="sessions-table">
                <thead
                  ><tr
                    ><th>{t.conversations}</th><th class="numeric" aria-label={t.usageColumn}><span class="usage-column-label">{t.usageColumn}</span><span class="usage-column-short">{t.tokens}</span></th><th
                      class="numeric"
                      aria-sort={query.sort === 'cost'
                        ? query.ascending
                          ? 'ascending'
                          : 'descending'
                        : undefined}
                      ><span class="column-label" class:ascending={query.ascending}
                        >{t.cost}{#if query.sort === 'cost'}<Icon
                            name="arrow"
                            size={16}
                          />{/if}</span
                      ></th
                    ><th class="numeric cache-column">{t.cacheHit}</th></tr
                  ></thead
                >
                <tbody
                  >{#each listing.page.items as Session[] as row (row.uid)}
                    <tr class:selected={selected === row.uid} data-uid={row.uid}
                      ><td
                        ><div class="title-cell">
                          <span class="session-icon"><Icon name="message" size={18} /></span>
                          <button
                            class="session-title"
                            onclick={() => openDetail(row.uid)}
                            ><span class="conversation-title-line"><span class="title-text" data-tooltip={row.title || row.session_id} data-tooltip-overflow>{row.title || row.session_id}</span><RelativeTime value={activityTime(row)} {lang} zone={query.timezone} /></span><small
                              class="conversation-meta"
                              >{#if showEnvironment}<span class="environment-badge" data-environment={row.environment}>{row.environment}</span>{/if}<ArchiveTag source={row.source} {lang} /><span class="conversation-project" data-tooltip={row.project} data-tooltip-overflow>{row.project}</span>{#if row.is_subagent}<span class="role-tag">{t.agentLabel}</span>{/if}</small
                              ><SessionMetadata sessions={[row]} {lang} /></button
                          >
                        </div></td
                      >
                      <td class="numeric" data-tooltip={usageSpeedHint(row.estimated_speed, lang)}
                        ><AnimatedValue
                          value={row.total_token_usage?.total_tokens ?? 0}
                          format={compactNumber}
                        /></td
                      >
                      <td class="numeric cost-cell" data-tooltip={money(row.estimated_cost_usd, lang)}
                        ><button
                          class="session-open"
                          onclick={() => openDetail(row.uid)}
                          aria-label={`${t.details}: ${row.title || row.session_id}`}
                          ><span
                            >{#if row.estimated_cost_usd == null}{t.unknown}{:else}<AnimatedValue
                                value={row.estimated_cost_usd}
                                format={displayMoney}
                              />{/if}</span
                          ><Icon name="chevron" size={17} /></button
                        ></td
                      ><td class="numeric cache-cell"><CacheHit usage={row.total_token_usage} {lang} /></td></tr
                    >
                  {/each}</tbody
                >
              </table>
            </div>
          {/if}
        </div>
        {#if listing && displayedMode === 'sessions' && (listing.page.next_cursor || listing.page.previous_cursor)}<Pager
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
          {showEnvironment}
          projectView={mode === 'projects'}
          {bundle}
          loading={detailLoading}
          error={detailError}
          retryable={detailRetryable}
          {lang}
          zone={query.timezone}
          {data}
          close={closeDetail}
          retry={() => openDetail(selected, false)}
          active={page === 'overview'}
          onerror={childError}
        />
      {:else}<aside class="detail-placeholder">
          <span class="placeholder-art"><Icon name="message" size={36} /></span>
          <h2>{t.selectSession}</h2>
        </aside>{/if}
    </div>
    </div>
    </div>
    <footer>
      <span><Icon name="shield" size={14} />{t.localOnly} · {t.estimatedHint}</span>
    </footer>
  </main>
</div>
