<script lang="ts">
  import type { Detail } from '../lib/data/types';
  import { words, number, type Language } from '../lib/domain/ui';
  import { usageBreakdown, usagePriceTooltip } from '../lib/domain/usageBreakdown';
  import { displayMoney } from '../lib/domain/presentation';
  import { metricTooltip } from '../lib/domain/metricTooltip';
  import AnimatedValue from './AnimatedValue.svelte';

  let { detail, lang, zone }: { detail: Detail; lang: Language; zone: string } = $props();
  let t = $derived(words(lang));
  let parts = $derived(usageBreakdown(detail));
  let maximum = $derived(Math.max(1, ...parts.map(part => part.tokens)));
</script>

<div class="usage-breakdown" style:--breakdown-rows={parts.length} role="group" aria-label={t.breakdown}>
  {#each parts as part, i (part.key)}
    <span class="breakdown-label" class:nested={part.nested} style:grid-row={i + 1}>{part.nested ? t.reasoningSubset : t[part.key]}</span>
    <span class="breakdown-track" class:nested={part.nested} style:grid-row={i + 1} aria-hidden="true"><span
      style:width={`${part.tokens / maximum * 100}%`} style:background={part.color}
    ></span></span>
    <span class="breakdown-count" class:nested={part.nested} style:grid-row={i + 1}><AnimatedValue value={part.tokens} format={value => number(Math.round(value), lang)} /></span>
  {/each}
  <button type="button" class="breakdown-prices" use:metricTooltip={usagePriceTooltip(detail, lang, zone)} aria-label={t.prices}>
    {#each parts as part (part.key)}
      <strong class:nested={part.nested}>{#if part.cost === null}{t.unknown}{:else}<AnimatedValue value={part.cost} format={displayMoney} />{/if}</strong>
    {/each}
  </button>
</div>

<style>
  .usage-breakdown {
    --breakdown-row-height: 28px;
    display: grid;
    grid-template-columns: auto minmax(24px, 1fr) auto auto;
    grid-template-rows: repeat(var(--breakdown-rows), var(--breakdown-row-height));
    align-items: center;
    column-gap: 10px;
    margin: 20px 0 24px;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
  .breakdown-label { grid-column: 1; }
  .breakdown-label.nested { position: relative; padding-left: 16px; }
  .breakdown-label.nested::before {
    content: '';
    position: absolute;
    left: 4px;
    bottom: 50%;
    width: 8px;
    height: calc(var(--breakdown-row-height) - 10px);
    border-left: 1px solid #bdd3c7;
    border-bottom: 1px solid #bdd3c7;
    border-bottom-left-radius: 4px;
    pointer-events: none;
  }
  .breakdown-label.nested,
  .breakdown-count.nested { font-size: .92em; color: #819286; }
  .breakdown-track {
    grid-column: 2;
    height: 6px;
    overflow: hidden;
    border-radius: 5px;
    background: #ebefed;
  }
  .breakdown-track > span { display: block; height: 100%; border-radius: inherit; }
  .breakdown-track.nested { height: 4px; opacity: .55; }
  .breakdown-count { grid-column: 3; text-align: right; }
  .breakdown-prices {
    border: 0;
    background: transparent;
    font: inherit;
    letter-spacing: inherit;
    grid-column: 4;
    grid-row: 1 / span var(--breakdown-rows);
    align-self: stretch;
    display: grid;
    grid-template-rows: repeat(var(--breakdown-rows), var(--breakdown-row-height));
    gap: 0;
    min-height: 0;
    align-items: center;
    justify-content: stretch;
    padding: 0 8px;
    margin-left: -8px;
    margin-top: 0;
    margin-bottom: 0;
    margin-right: 0;
    cursor: help;
    border-radius: 6px;
  }
  .breakdown-prices strong { color: var(--accent-strong); text-align: right; font-weight: 600; }
  .breakdown-prices strong.nested { font-size: .92em; font-weight: 400; color: #819286; }
  .breakdown-prices:hover,
  .breakdown-prices:focus-visible { background: #edf8f3; }
  .breakdown-prices:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .breakdown-prices:active { transform: none; }
  @media (max-width: 480px) {
    .usage-breakdown { --breakdown-row-height: 26px; column-gap: 6px; font-size: 11px; }
  }
</style>
