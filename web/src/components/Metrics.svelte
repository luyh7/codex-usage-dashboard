<script lang="ts">
  import type { Summary } from '../lib/data/types';
  import { words, number, money, type Language } from '../lib/domain/ui';
  import { compactNumber, displayMoney } from '../lib/domain/presentation';
  import AnimatedValue from './AnimatedValue.svelte';
  import Icon from './Icon.svelte';
  let { summary, lang }: { summary: Summary | null; lang: Language } = $props();
  let t = $derived(words(lang));
  let usage = $derived(summary?.usage);
  let hit = $derived(
    usage?.input_tokens
      ? Math.min(100, Math.max(0, (usage.cached_input_tokens / usage.input_tokens) * 100))
      : null,
  );
  let cost = $derived(
    summary && (!summary.session_count || summary.estimated_cost_known_count)
      ? summary.estimated_cost_usd
      : null,
  );
</script>

<section class="metrics" aria-label={t.overview}>
  <article class="cost-card">
    <div class="metric-label">{t.cost}</div>
    <strong
      title={summary ? money(cost, lang) : undefined}
      style:--value-length={cost === null ? 8 : displayMoney(cost).length}
    >
      {#if summary && cost === null}{t.unknown}{:else}<AnimatedValue
          value={cost}
          format={displayMoney}
        />{/if}
    </strong>
    {#if summary && summary.estimated_cost_known_count < summary.session_count}<small
        class="estimate-note"
        title={t.unknownHint}>{t.partial}</small
      >{/if}
  </article>
  <article>
    <details class="metric-breakdown">
      <summary class="metric-label" title={t.breakdown}
        >{t.tokens}<Icon name="down" size={13} /></summary
      >
      <div class="breakdown-values">
        {#each [['input', usage?.input_tokens], ['cached', usage?.cached_input_tokens], ['cacheWrite', usage?.cache_write_tokens], ['output', usage?.output_tokens], ['reasoning', usage?.reasoning_output_tokens]] as [key, value]}<span
            >{t[key as keyof typeof t]}<b>{summary ? number(value as number, lang) : '—'}</b></span
          >{/each}
      </div>
    </details>
    <div class="metric-value">
      <strong title={number(usage?.total_tokens, lang)}
        ><AnimatedValue
          value={summary ? (usage?.total_tokens ?? 0) : null}
          format={compactNumber}
        /></strong
      ><span class="metric-unit">tokens</span>
    </div>
  </article>
  <article>
    <div class="metric-label">{t.sessions}</div>
    <strong
      title={`${t.active}: ${number(summary?.active_count, lang)} / ${t.archived}: ${number(summary?.archived_count, lang)}`}
      ><AnimatedValue
        value={summary?.session_count ?? null}
        format={(value) => number(Math.round(value), lang)}
      /></strong
    >
  </article>
  <article class="cache-card">
    <div class="metric-label">{t.cache}</div>
    <div class="cache-value">
      <strong><AnimatedValue value={hit} format={(value) => number(value, lang) + '%'} /></strong
      ><svg class="cache-ring" viewBox="0 0 44 44" aria-hidden="true"
        ><defs
          ><linearGradient id="cache-gradient"
            ><stop offset="0%" stop-color="#bde8d9" /><stop
              offset="100%"
              stop-color="#00a165"
            /></linearGradient
          ></defs
        ><circle cx="22" cy="22" r="17" class="ring-track" /><circle
          cx="22"
          cy="22"
          r="17"
          class="ring-value"
          pathLength="100"
          stroke-dasharray={`${hit ?? 0} 100`}
        /></svg
      >
    </div>
  </article>
</section>
