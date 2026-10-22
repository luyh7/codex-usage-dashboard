<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import { filterValues, filterSelection } from '../lib/domain/filterSelection';
  import type { Query, Source } from '../lib/data/types';
  import { words, type Language } from '../lib/domain/ui';

  let { query, lang, sources, modelOptions, change, clear, apply, dateControls }: {
    query: Query;
    lang: Language;
    sources: Source[];
    modelOptions: string[];
    change: <K extends keyof Query>(key: K, value: Query[K]) => void;
    clear: () => void;
    apply: (filters: Pick<Query, FilterKey>) => void;
    dateControls: Snippet;
  } = $props();
  type FilterKey = 'environment' | 'source' | 'model' | 'agents';
  type FilterGroup = { key: FilterKey; label: string; multiple?: boolean; options: [string, string][] };
  let t = $derived(words(lang));
  let open = $state(false);
  let draft = $state<Pick<Query, FilterKey>>({ environment: '', source: '', model: '', agents: '' });
  let hasDraftConditions = $derived(Object.values(draft).some(Boolean));
  let trigger: HTMLButtonElement;
  let panel: HTMLDivElement;
  let search: HTMLInputElement;
  let groups = $derived<FilterGroup[]>([
    { key: 'environment', label: t.environment, multiple: true, options: [['', t.allEnvironments], ...sources.map(s => [s.id, s.label] as [string, string])] },
    { key: 'source', label: t.source, options: [['', t.all], ['active', t.active], ['archived', t.archived]] },
    { key: 'agents', label: t.agents, options: [['', t.all], ['main', t.main], ['agents', t.onlyAgents]] },
    { key: 'model', label: t.model, multiple: true, options: [['', t.allModels], ...[...new Set([...modelOptions, ...filterValues(query.model)].filter(Boolean))].map(m => [m, m] as [string, string])] },
  ]);
  let active = $derived(groups.flatMap(group => selectedValues(group).map(value => ({
    key: group.key, label: group.label, value,
    text: group.options.find(([option]) => option === value)?.[1] || value,
  }))));
  function selectedValues(group: FilterGroup, values: Pick<Query, FilterKey> = query): string[] {
    return group.multiple ? filterValues(values[group.key]) : values[group.key] ? [values[group.key]] : [];
  }
  function isSelected(group: FilterGroup, value: string): boolean {
    const selected = selectedValues(group, draft);
    return value ? selected.includes(value) : selected.length === 0;
  }
  function choose(group: FilterGroup, value: string) {
    if (!group.multiple || !value) { draft = { ...draft, [group.key]: value }; return; }
    const selected = selectedValues(group, draft);
    draft = { ...draft, [group.key]: filterSelection(selected.includes(value) ? selected.filter(item => item !== value) : [...selected, value]) };
  }
  function remove(key: FilterKey, value: string) {
    const group = groups.find(group => group.key === key)!;
    change(key, group.multiple ? filterSelection(selectedValues(group).filter(item => item !== value)) : '');
  }
  let hasConditions = $derived(!!query.search || active.length > 0);

  function position() {
    if (!open || !panel) return;
    const anchor = trigger.getBoundingClientRect();
    const width = Math.min(460, innerWidth - 24);
    panel.style.width = `${width}px`;
    panel.style.maxHeight = `${Math.max(100, innerHeight - anchor.bottom - 24)}px`;
    panel.style.left = `${Math.max(12, Math.min(innerWidth - width - 12, anchor.right - width))}px`;
    panel.style.top = `${anchor.bottom + 8}px`;
  }
  function beginDraft() {
    draft = { environment: query.environment, source: query.source, model: query.model, agents: query.agents };
    position();
  }
  function confirm() {
    if ((Object.keys(draft) as FilterKey[]).some(key => draft[key] !== query[key])) apply({ ...draft });
    close();
  }
  function clearDraft() {
    draft = { environment: '', source: '', model: '', agents: '' };
  }
  function close() {
    panel.hidePopover();
    trigger.focus({ preventScroll: true });
  }
  function keyboard(event: KeyboardEvent) {
    if (!open || event.key !== 'Escape') return;
    event.preventDefault();
    event.stopImmediatePropagation();
    close();
  }
  function clearSearch() {
    change('search', '');
    search.focus({ preventScroll: true });
  }
</script>

<svelte:window onresize={position} onscrollcapture={position} onkeydowncapture={keyboard} />

<section class="global-filters" aria-label={t.globalFilters}>
  <div class="global-toolbar">
    <label class="global-search">
      <Icon name="search" size={19} />
      <input bind:this={search} type="search" aria-label={t.search} placeholder={t.projectSearch} value={query.search} oninput={(event) => change('search', event.currentTarget.value)} />
      {#if query.search}<button class="search-clear" aria-label={t.clearSearch} onclick={clearSearch}>×</button>{/if}
    </label>
    <button bind:this={trigger} class="filter-toggle" class:chosen={open || active.length > 0} popovertarget="global-filter-panel" aria-label={t.filters + (active.length ? ' · ' + active.length : '')} aria-expanded={open} aria-haspopup="dialog" aria-controls="global-filter-panel">
      <Icon name="filter" size={18} /><span>{t.filters}</span>
      {#if active.length}<span class="filter-count">{active.length}</span>{/if}
      <span class="filter-chevron" class:expanded={open}><Icon name="down" size={13} /></span>
    </button>
    {@render dateControls()}
  </div>
  {#if hasConditions}
    <div class="active-filters" aria-label={t.selected}>
      {#if query.search}<button class="filter-chip" aria-label={`${t.removeFilter}: ${t.search}: ${query.search}`} onclick={clearSearch}><span>{t.search}: {query.search}</span><span aria-hidden="true">×</span></button>{/if}
      {#each active as item (JSON.stringify([item.key, item.value]))}
        <button class="filter-chip" aria-label={`${t.removeFilter}: ${item.label}: ${item.text}`} onclick={() => remove(item.key, item.value)}><span>{item.label}: {item.text}</span><span aria-hidden="true">×</span></button>
      {/each}
      <button class="clear-filters" onclick={clear}>{t.clear}</button>
    </div>
  {/if}
</section>

<div bind:this={panel} id="global-filter-panel" class="global-filter-panel" popover="auto" role="dialog" aria-label={t.filters} onbeforetoggle={(event) => { open = event.newState === 'open'; if (open) beginDraft(); }}>
  <button class="close-filters" aria-label={t.close} onclick={close}>×</button>
  <div class="filter-groups">
    {#each groups as group (group.key)}
      <div class="filter-group" role="group" aria-label={group.label}>
        <span class="group-label">{group.label}{#if group.multiple}<small>{t.multipleSelect}</small>{/if}</span>
        <div class="filter-options">
          {#each group.options as [value, label] (value)}
            <button class:chosen={isSelected(group, value)} aria-pressed={isSelected(group, value)} onclick={() => choose(group, value)}>{#if isSelected(group, value)}<Icon name="check" size={13} />{/if}{label}</button>
          {/each}
        </div>
      </div>
    {/each}
  </div>
  <footer class="filter-footer"><button disabled={!hasDraftConditions} onclick={clearDraft}>{t.clear}</button><button class="primary confirm-filters" onclick={confirm}>{t.applyFilters}</button></footer>
</div>

<style>
  .global-filters { flex: 1; min-width: 0; }
  .global-toolbar { display: flex; align-items: center; min-height: 50px; gap: 10px; }
  .global-search { display: flex; flex: 0 1 320px; align-items: center; min-width: 160px; height: 40px; gap: 9px; padding: 0 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--panel); color: var(--muted); }
  .global-search:focus-within { outline: 2px solid var(--accent); outline-offset: 1px; }
  .global-search :global(.ui-icon) { flex-shrink: 0; }
  .global-search input { min-width: 0; width: 100%; min-height: 0; padding: 0; border: 0; border-radius: 0; background: transparent; font-size: 13px; color: var(--text); }
  .global-search input:focus-visible { outline: none; }
  .global-search input::-webkit-search-cancel-button { appearance: none; }
  .search-clear, .close-filters { display: grid; place-items: center; flex-shrink: 0; border: 0; background: transparent; padding: 0; min-height: 28px; min-width: 28px; color: var(--muted); font-size: 20px; }
  .filter-toggle { display: flex; flex-shrink: 0; align-items: center; gap: 8px; min-height: 40px; padding: 7px 12px; border-radius: 10px; font-size: 13px; }
  .filter-toggle.chosen { border-color: #afe0cf; color: var(--accent-strong); background: var(--accent-bg); }
  .filter-count { display: grid; place-items: center; min-width: 19px; height: 19px; border-radius: 6px; background: var(--panel); font-size: 11px; font-weight: 600; }
  .filter-chevron { display: flex; transition: transform 150ms; }
  .filter-chevron.expanded { transform: rotate(180deg); }
  .global-toolbar :global(.date-controls) { min-width: 0; width: auto; margin-left: auto; gap: 10px; }
  .global-toolbar :global(.period-tabs) { height: 40px; padding: 3px; border-radius: 10px; }
  .global-toolbar :global(.period-tabs > button) { min-height: 32px; padding: 4px 14px; font-size: 12px; }
  .global-toolbar :global(.calendar-trigger) { min-height: 40px; border-radius: 10px; font-size: 12px; }
  .global-toolbar :global(.mobile-period) { min-height: 0; }
  .active-filters { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; margin-top: 9px; max-height: 72px; overflow: auto; }
  .filter-chip { display: flex; align-items: center; gap: 9px; min-width: 0; max-width: 100%; min-height: 28px; padding: 4px 9px; border: 0; border-radius: 8px; background: var(--accent-bg); color: var(--accent-strong); font-size: 11px; }
  .filter-chip > span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .filter-chip:hover { background: #d5f1e6; }
  .clear-filters { min-height: 28px; padding: 4px 7px; border: 0; background: transparent; color: var(--muted); font-size: 11px; }
  .global-filter-panel { position: fixed; inset: auto; margin: 0; padding: 0; width: min(460px, calc(100vw - 24px)); overflow: hidden; overscroll-behavior: contain; border: 1px solid var(--border); border-radius: 14px; background: var(--panel); color: var(--text); box-shadow: 0 12px 40px #16263f22; }
  .global-filter-panel:popover-open { display: flex; flex-direction: column; }
  .close-filters { position: absolute; top: 10px; right: 10px; z-index: 1; border-radius: 6px; background: var(--panel); }
  .filter-groups { display: grid; gap: 18px; padding: 20px 18px 16px; min-height: 0; overflow: auto; overscroll-behavior: contain; }
  .filter-group:first-child .group-label { padding-right: 32px; }
  .filter-group { display: grid; gap: 8px; }
  .group-label { font-size: 12px; font-weight: 600; }
  .group-label small { margin-left: 8px; color: var(--muted); font-size: 10px; font-weight: 400; }
  .filter-options { display: flex; flex-wrap: wrap; gap: 6px; }
  .filter-options button { display: flex; align-items: center; gap: 5px; min-height: 32px; padding: 5px 10px; max-width: 100%; overflow-wrap: anywhere; border: 1px solid var(--border); border-radius: 8px; background: var(--panel); color: var(--muted); font-size: 12px; }
  .filter-options button.chosen { border-color: #afe0cf; color: var(--accent-strong); background: var(--accent-bg); }
  .filter-options button :global(.ui-icon) { flex-shrink: 0; }
  .filter-options button:hover { border-color: var(--accent); }
  .filter-footer { flex-shrink: 0; display: flex; align-items: center; justify-content: flex-end; gap: 12px; margin: 0; padding: 12px 18px; border-top: 1px solid var(--border); font-size: 11px; color: var(--muted); }
  .filter-footer button { flex-shrink: 0; min-height: 32px; padding: 5px 12px; font-size: 12px; }
  .filter-footer .confirm-filters { min-width: 72px; }
  @media (max-width: 1200px) {
    .global-toolbar :global(.period-tabs) { display: none; }
    .global-toolbar :global(.mobile-period) { display: block; width: 112px; }
  }
  @media (max-width: 1000px) {
    .global-filters { flex: 0 0 auto; width: 100%; }
    .global-toolbar { flex-wrap: wrap; }
    .global-search { flex: 1; }
    .global-toolbar :global(.date-controls) { flex: 1 1 100%; margin-left: 0; }
    .global-toolbar :global(.calendar-anchor) { flex: 1; min-width: 0; }
    .global-toolbar :global(.calendar-trigger) { width: 100%; min-width: 0; }
  }
  @media (max-width: 760px) {
    .global-toolbar { gap: 8px; min-height: 0; }
    .global-search { min-width: 0; height: 38px; padding: 0 9px; gap: 7px; }
    .global-search input { font-size: 12px; }
    .filter-toggle { min-height: 38px; gap: 6px; padding: 6px 10px; font-size: 12px; }
    .global-toolbar :global(.date-controls) { gap: 8px; }
    .global-toolbar :global(.mobile-period) { width: 100px; }
    .global-toolbar :global(.calendar-trigger) { min-height: 38px; font-size: 11px; }
    .filter-groups { padding: 18px 14px 14px; gap: 16px; }
    .filter-options button { min-height: 36px; }
    .filter-footer { padding: 10px 14px; }
  }
</style>
