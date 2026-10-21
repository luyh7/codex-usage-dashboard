<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import type { DashboardData, Project, Query, Session, Page, TaskGroup } from '../lib/data/types';
  import { cancelled } from '../lib/data/http';
  import { words, number, money, type Language } from '../lib/domain/ui';
  import TaskRows from './TaskRows.svelte';
  import Pager from './Pager.svelte';
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
  let loading = $state(false);
  let error = $state('');
  let loaded = '';
  let loadedQuery = '';
  let sequence = 0;
  let alive = true;
  async function load(cursor = '', q = query, r = revision, offset = 0) {
    const seq = ++sequence;
    loading = true;
    error = '';
    try {
      const result = await data.projectTasks({ ...q, offset }, project.key, r, cursor);
      if (seq === sequence && alive) {
        page = result;
        loaded = JSON.stringify([q, r]);
        loadedQuery = JSON.stringify(q);
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
      untrack(() => void load('', q, r, JSON.stringify(q) === loadedQuery ? page?.offset || 0 : 0));
    }
  });
  onDestroy(() => {
    alive = false;
    sequence++;
  });
</script>

<section class="project-block">
  <button class="project-heading" aria-expanded={open} onclick={toggle}
    ><span class="folder-icon">{open ? '▾' : '▸'} ▱</span><span class="project-name"
      >{project.label || project.key}<small title={project.key}
        >{project.key} · {project.environments.join(' / ')}</small
      ></span
    ><span class="numeric">{number(project.usage.total_tokens, lang)} <small>tokens</small></span
    ><span class="numeric project-cost"
      >{project.estimated_cost_known_count
        ? money(project.estimated_cost_usd, lang)
        : t.unknown}<small>{number(project.session_count, lang)} {t.sessions}</small></span
    ></button
  >
  {#if open}<div class="project-content" aria-busy={loading}>
      {#if error}<p class="error">
          {error} <button onclick={() => load()}>{t.retry}</button>
        </p>{/if}{#if loading && !page}<p class="muted" role="status">
          {t.loading}
        </p>{/if}{#if page}<div class="table-scroll">
          <table>
            <thead
              ><tr
                ><th>{t.conversations}</th><th class="numeric">Tokens</th><th class="numeric"
                  >{t.cost}</th
                ><th class="numeric">{t.turns}</th></tr
              ></thead
            ><tbody
              >{#each page.items as group (group.identity)}<TaskRows
                  {group}
                  {selected}
                  {select}
                  {lang}
                  expanded={agentExpanded.has(group.identity)}
                  toggle={() => toggleAgent(group.identity)}
                />{/each}</tbody
            >
          </table>
        </div>
        <Pager
          offset={page.offset}
          total={page.total}
          limit={page.limit}
          next={page.next_cursor}
          previous={page.previous_cursor}
          disabled={loading}
          change={(cursor) => load(cursor)}
          {lang}
        />{/if}
    </div>{/if}
</section>
