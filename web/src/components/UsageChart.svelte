<script lang="ts">
  import type { Chart } from '../lib/data/types';
  import { chart, type ChartPoint } from '../lib/domain/chart';
  import { compactNumber } from '../lib/domain/presentation';
  import { words, type Language } from '../lib/domain/ui';
  let { data, lang, zone }: { data: Chart; lang: Language; zone: string } = $props();
  let point = $state<ChartPoint | null>(null);
  let surfaceWidth = $state(400);
  let tooltipWidth = $state(168);
  let tooltipHeight = $state(80);
  let t = $derived(words(lang));
  const inspect = (next: ChartPoint | null) => {
    point = next;
  };
</script>

<figure class="usage-chart">
  <figcaption>
    <h3>{t.usageTrend}</h3>
    <span class="usage-legend"
      ><span><i class="line-key"></i>{t.cumulative}</span><span
        ><i class="bar-key"></i>{t.intervalTokens}</span
      ></span
    >
  </figcaption>
  {#if data.timeline.length}
    <div class="chart-surface" bind:clientWidth={surfaceWidth}>
      <canvas
        use:chart={{ data, lang, zone, inspect }}
        tabindex="0"
        role="slider"
        aria-label={t.curve + '. ' + t.chartHint}
        aria-valuemin="1"
        aria-valuemax={data.timeline.length}
        aria-valuenow={data.timeline.length}
      ></canvas>
      {#if point}<div
          class="chart-tooltip"
          role="tooltip"
          bind:offsetWidth={tooltipWidth}
          bind:offsetHeight={tooltipHeight}
          style:left={Math.max(0, Math.min(surfaceWidth - tooltipWidth, point.x - tooltipWidth / 2)) + 'px'}
          style:top={Math.max(-38, point.y - tooltipHeight - 10) + 'px'}
        >
          <strong>{compactNumber(point.tokens)} <small>tokens</small></strong><span
            >{point.time}</span
          ><span>{t.intervalTokens} <b>{compactNumber(point.intervalTokens)}</b></span>
        </div>{/if}
    </div>
    <p class="sr-only">{t.chartHint}</p>
  {:else}<p class="chart-empty">{t.noEvents}</p>{/if}
</figure>

<style>
  .usage-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-left: auto;
    color: var(--muted);
    font-size: 10px;
  }
  .usage-legend > span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .line-key {
    width: 14px;
    height: 2px;
    background: var(--accent);
    border-radius: 2px;
  }
  .bar-key {
    width: 7px;
    height: 9px;
    background: var(--accent);
    opacity: 0.3;
    border-radius: 1px;
  }
  .chart-tooltip {
    min-width: 148px;
    max-width: min(220px, 100%);
  }
  .chart-tooltip b {
    color: #264b40;
    font-weight: 600;
  }
</style>
