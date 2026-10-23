<script lang="ts">
  import { words, number, type Language } from '../lib/domain/ui';
  let {
    offset,
    total,
    limit,
    next,
    previous,
    disabled = false,
    change,
    lang,
  }: {
    offset: number;
    total: number;
    limit: number;
    next: string | null;
    previous: string | null;
    disabled?: boolean;
    change: (cursor: string) => void;
    lang: Language;
  } = $props();
  let t = $derived(words(lang));
</script>

<nav class="pager" aria-label={t.show}>
  <span aria-live="polite"
    >{number(total ? offset + 1 : 0, lang)}–{number(Math.min(offset + limit, total), lang)} / {number(
      total,
      lang,
    )}</span
  ><button disabled={disabled || !previous} onclick={() => previous && change(previous)}
    >{t.previous}</button
  ><button disabled={disabled || !next} onclick={() => next && change(next)}>{t.next}</button>
</nav>
