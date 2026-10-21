<script lang="ts">
  import type { Summary } from '../lib/data/types';
  import { words, number, money, type Language } from '../lib/domain/ui';
  let { summary, lang }: { summary: Summary | null; lang: Language } = $props();
  let t = $derived(words(lang));
  let usage = $derived(summary?.usage);
  let hit = $derived(
    usage?.input_tokens ? (usage.cached_input_tokens / usage.input_tokens) * 100 : null,
  );
</script>

<section class="metrics" aria-label={t.breakdown}>
  <article>
    <span>{t.tokens}</span><strong>{summary ? number(usage?.total_tokens, lang) : '—'}</strong
    ><small>{t.localOnly}</small>
  </article>
  <article>
    <span>{t.cost}</span><strong
      >{summary?.estimated_cost_known_count
        ? money(summary.estimated_cost_usd, lang)
        : summary
          ? t.unknown
          : '—'}</strong
    ><small title={t.unknownHint}
      >{summary && summary.estimated_cost_known_count < summary.session_count
        ? t.partial
        : t.estimatedHint}</small
    >
  </article>
  <article>
    <span>{t.sessions}</span><strong>{summary ? number(summary.session_count, lang) : '—'}</strong
    ><small
      >{t.active}
      {number(summary?.active_count, lang)} · {t.archived}
      {number(summary?.archived_count, lang)}</small
    >
  </article>
  <article>
    <span>{t.cache}</span><strong>{hit === null ? '—' : number(hit, lang) + '%'}</strong><small
      >{t.cached} / {t.input}</small
    >
  </article>
</section>
<details class="breakdown">
  <summary>{t.breakdown}</summary>
  <div class="breakdown-values">
    {#each [['input', usage?.input_tokens], ['cached', usage?.cached_input_tokens], ['cacheWrite', usage?.cache_write_tokens], ['output', usage?.output_tokens], ['reasoning', usage?.reasoning_output_tokens]] as [key, value]}<span
        >{t[key as keyof typeof t]} <b>{number(value as number, lang)}</b></span
      >{/each}
  </div>
</details>
