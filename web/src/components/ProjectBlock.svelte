<script lang="ts">
  import { onDestroy, tick, untrack } from 'svelte';
  import type { DashboardData, Project, Query, Session, Page, TaskGroup } from '../lib/data/types';
  import { cancelled } from '../lib/data/http';
  import { words, number, money, type Language } from '../lib/domain/ui';
  import Icon from './Icon.svelte';
  import AnimatedValue from './AnimatedValue.svelte';
  import { compactNumber, displayMoney } from '../lib/domain/presentation';
  import TaskRows from './TaskRows.svelte';
  import RelativeTime from './RelativeTime.svelte';
  import CacheHit from './CacheHit.svelte';
  let {
    project,
    query,
    revision,
    data,
    open,
    toggle,
    selected,
    select,
    lang,
    agentExpanded,
    toggleAgent,
    onerror,
  }: {
    project: Project;
    query: Query;
    revision: string;
    data: DashboardData;
    open: boolean;
    toggle: () => void;
    selected: string;
    select: (row: Session) => void;
    lang: Language;
    agentExpanded: Set<string>;
    toggleAgent: (id: string) => void;
    onerror: (error: unknown) => void;
  } = $props();
  let t = $derived(words(lang));
  let page = $state.raw<Page<TaskGroup> | null>(null);
  const previewLimit = 5;
  let showAll = $state(false);
  let heading: HTMLButtonElement;
  let visibleTasks = $derived(showAll ? page?.items : page?.items.slice(0, previewLimit));
  let loading = $state(false);
  let error = $state('');
  let loaded = '';
  let loadedQuery = '';
  let sequence = 0;
  let alive = true;
  async function load(q = query, r = revision, all = showAll) {
    const seq = ++sequence;
    loading = true;
    error = '';
    try {
      let result = await data.projectTasks({ ...q, offset: 0 }, project.key, r, '', all ? 100 : previewLimit);
      const items = [...result.items];
      while (all && result.next_cursor) {
        if (seq !== sequence || !alive) return;
        result = await data.projectTasks({ ...q, offset: 0 }, project.key, r, result.next_cursor, 100);
        items.push(...result.items);
      }
      if (seq === sequence && alive) {
        page = { ...result, items, offset: 0, previous_cursor: null };
        loaded = JSON.stringify([q, r]);
        loadedQuery = JSON.stringify(q);
        return true;
      }
    } catch (e) {
      if (!cancelled(e) && seq === sequence && alive) {
        error = String(e);
        onerror(e);
      }
    } finally {
      if (seq === sequence && alive) loading = false;
    }
  }
  $effect(() => {
    const key = JSON.stringify([query, revision]);
    if (open && key !== loaded) {
      const q = query,
        r = revision;
      untrack(() => {
        if (JSON.stringify(q) !== loadedQuery) showAll = false;
        void load(q, r);
      });
    }
  });
  async function toggleTasks() {
    if (!showAll) {
      if (page && page.items.length < page.total) {
        if (!(await load(query, revision, true))) return;
      }
      showAll = true;
    } else {
      showAll = false;
      await tick();
      const scroller = heading.closest('.browser-scroll');
      if (scroller) {
        const top = scroller.getBoundingClientRect().top + 31;
        const delta = heading.getBoundingClientRect().top - top;
        if (delta < 0) scroller.scrollBy({ top: delta, behavior: 'instant' });
      }
    }
  }
  onDestroy(() => {
    alive = false;
    sequence++;
  });
</script>

<section class="project-block" class:open>
  <button bind:this={heading} class="project-heading project-row" aria-expanded={open} onclick={toggle}>
    <span class="project-title"
      ><span class="folder-icon"
        ><span class:expanded={open}><Icon name="chevron" size={14} /></span><Icon
          name="folder"
          size={28}
        /></span
      ><span class="project-name"
        ><span class="conversation-title-line"><span class="title-text" data-tooltip={project.label || project.key} data-tooltip-overflow>{project.label || project.key}</span><span class="project-environments">{#each project.environments as environment}<span class="environment-badge" data-environment={environment}>{environment}</span>{/each}</span></span><small class="project-meta"
          ><AnimatedValue
            value={project.session_count}
            format={(value) => number(Math.round(value), lang)}
          />
          {t.sessionCount}<span><RelativeTime value={project.updated_at} {lang} zone={query.timezone} /></span></small
        ></span
      ></span
    >
    <span class="numeric" data-tooltip={number(project.usage.total_tokens, lang)}
      ><AnimatedValue value={project.usage.total_tokens} format={compactNumber} /></span
    >
    <span class="numeric project-cost" data-tooltip={money(project.estimated_cost_usd, lang)}
      >{#if project.estimated_cost_known_count}<AnimatedValue
          value={project.estimated_cost_usd}
          format={displayMoney}
        />{:else}{t.unknown}{/if}</span
    >
    <span class="numeric cache-cell"><CacheHit usage={project.usage} {lang} /></span>
  </button>
  {#if open}<div class="project-content" aria-busy={loading}>
      {#if error}<p class="error">{error} <button onclick={() => load()}>{t.retry}</button></p>{/if}
      {#if loading && !page}<p class="muted" role="status">{t.loading}</p>{/if}
      {#if page}
        <div class="project-tasks">
          {#each visibleTasks ?? [] as group (group.identity)}<TaskRows
              zone={query.timezone}
              {group}
              {selected}
              {select}
              {lang}
              expanded={agentExpanded.has(group.identity)}
              toggle={() => toggleAgent(group.identity)}
            />{/each}
        </div>
        {#if page.total > previewLimit}<button
            class="project-more"
            aria-expanded={showAll}
            disabled={loading}
            onclick={toggleTasks}
          >{#if loading}{t.loading}{:else if showAll}{t.collapseProjectTasks}{:else}{t.expandProjectTasks.replace('{count}', number(page.total - previewLimit, lang))}{/if}</button>{/if}
      {/if}
    </div>{/if}
</section>
