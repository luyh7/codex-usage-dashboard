<script lang="ts">
  import Dropdown from './Dropdown.svelte';
  import Icon from './Icon.svelte';
  import { words, type Language } from '../lib/domain/ui';
  let {
    value,
    ascending,
    lang,
    change,
    reverse,
  }: {
    value: string;
    ascending: boolean;
    lang: Language;
    change: (value: string) => void;
    reverse: () => void;
  } = $props();
  let t = $derived(words(lang));
  let options = $derived<[string, string][]>([
    ['updated', t.updated],
    ['cost', t.sortCost],
    ['tokens', t.tokens],
    ['title', t.name],
  ]);
</script>

<div class="sorting">
  <button
    type="button"
    class="sort-direction"
    aria-label={ascending ? t.ascending : t.descending}
    aria-pressed={ascending}
    data-tooltip={ascending ? t.ascending : t.descending}
    onclick={reverse}
  >
    <Icon name={ascending ? 'sort-ascending' : 'sort-descending'} size={18} />
  </button>
  <Dropdown class="sort-menu" variant="sort" label={t.sort} {value} {options} {change} />
</div>

<style>
  .sorting {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--panel);
  }
  .sort-direction {
    display: flex;
    justify-content: center;
    align-items: center;
    border: 0;
    border-radius: 8px 0 0 8px;
    background: transparent;
    padding: 8px;
    min-height: 36px;
    min-width: 31px;
  }
  .sort-direction:hover {
    color: var(--accent-strong);
    background: var(--accent-bg);
  }
  .sort-direction:active :global(.ui-icon) {
    transform: scale(.9);
  }
  .sort-direction:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  @media (max-width: 760px) {
    .sort-direction {
      min-width: 28px;
      padding: 5px;
    }
  }
</style>
