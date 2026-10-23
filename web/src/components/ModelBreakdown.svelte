<script lang="ts">
  import type { Summary } from '../lib/data/types';
  import { words, number, type Language } from '../lib/domain/ui';
  import { compactNumber, displayMoney } from '../lib/domain/presentation';
  import { modelRows } from '../lib/domain/modelBreakdown';
  let { summary, lang, metric, start = 0, limit = Infinity, wide = false }: {
    summary: Summary | null; lang: Language; metric: 'tokens' | 'cost'; start?: number; limit?: number; wide?: boolean;
  } = $props();
  let rows = $derived(modelRows(summary?.model_breakdown ?? [], metric).slice(start, start + limit));
  let t = $derived(words(lang));
</script>

<div class="model-totals" class:wide role="region" aria-label={lang === 'zh' ? '各模型用量与费用' : 'Usage and cost for each model'}>
  {#each rows as row (row.model)}
    <section class="model-card" aria-label={row.model === 'unknown' ? t.unknown : row.model}>
      <h3 class="model-name"><i style:background={row.color}></i><span>{row.model === 'unknown' ? t.unknown : row.model}</span></h3>
      <div class="model-values">
        <b class="model-cost" aria-label={`${t.cost}: ${row.estimated_cost_usd === null ? t.unknown : displayMoney(row.estimated_cost_usd)}${row.cost_complete ? '' : ` · ${t.partial}`}`}>
          {row.estimated_cost_usd === null ? t.unknown : displayMoney(row.estimated_cost_usd)}{#if !row.cost_complete && row.estimated_cost_usd !== null}<span class="partial">*</span>{/if}
        </b>
        <span class="model-usage" aria-label={`${t.tokens}: ${number(row.usage.total_tokens, lang)} tokens`}>{compactNumber(row.usage.total_tokens)}</span>
      </div>
    </section>
  {/each}
</div>

<style>
  .model-totals { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); grid-auto-rows: var(--model-card-height, 84px); gap: 10px; align-content: start; }
  .model-totals.wide { grid-template-columns: repeat(5, minmax(0, 1fr)); }
  .model-card { min-width: 0; min-height: 0; padding: 10px 12px; border: 1px solid var(--border); border-radius: 12px; background: var(--panel); container-type: inline-size; display: flex; flex-direction: column; justify-content: center; gap: 8px; }
  .model-name { display: flex; align-items: center; gap: 7px; min-width: 0; margin: 0; font-size: 12px; font-weight: 500; line-height: 18px; color: var(--muted); }
  .model-name > span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .model-name i { width: 6px; height: 6px; border-radius: 2px; flex-shrink: 0; }
  .model-values { display: flex; align-items: baseline; justify-content: space-between; flex-wrap: wrap; gap: 2px 6px; font-variant-numeric: tabular-nums; }
  .model-cost { font-size: clamp(16px, 11cqw, 23px); font-weight: 700; line-height: 1.15; letter-spacing: -.6px; color: var(--number); white-space: nowrap; }
  .model-usage { font-size: clamp(10px, 6.5cqw, 13px); line-height: 1.4; color: var(--text); white-space: nowrap; }
  .partial { color: var(--muted); margin-left: 2px; }
  @media (max-width: 1100px) { .model-totals.wide { grid-template-columns: repeat(3, minmax(0, 1fr)); } .model-card { padding: 8px 12px; gap: 5px; } }
  @media (max-width: 600px) {
    .model-totals, .model-totals.wide { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
    .model-card { padding: 7px 10px; gap: 3px; }
    .model-name { font-size: 11px; line-height: 16px; }
    .model-values { flex-direction: column; align-items: flex-start; gap: 1px; }
    .model-cost { font-size: 19px; }
    .model-usage { font-size: 11px; }
  }
</style>
