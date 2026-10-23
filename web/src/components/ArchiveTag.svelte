<script lang="ts">
  import { words, type Language } from '../lib/domain/ui';
  let { source, lang }: { source: string; lang: Language } = $props();
  let t = $derived(words(lang));
</script>

{#if source === 'archived'}<span class="archive-tag">{t.archivedTag}</span>{/if}

<style>
  .archive-tag {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    width: fit-content;
    padding: 0 6px;
    border: 1px solid #e3b887;
    border-radius: 6px;
    color: #b85f18;
    background: #fff6ec;
    font-size: 11px;
    font-weight: 400;
    line-height: 18px;
    white-space: nowrap;
  }
  @media (max-width: 760px) {
    .archive-tag { padding: 0 4px; font-size: 9px; line-height: 16px; }
  }
</style>
