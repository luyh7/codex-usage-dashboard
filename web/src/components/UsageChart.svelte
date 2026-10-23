<script lang="ts">
  import type { Chart } from '../lib/data/types';
  import { chart } from '../lib/domain/chart';
  import { words, type Language } from '../lib/domain/ui';
  let { data, lang, zone }: { data: Chart; lang: Language; zone: string } = $props();
  let t = $derived(words(lang));
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
    <div class="chart-surface">
      <canvas
        use:chart={{ data, lang, zone }}
        tabindex="0"
        role="slider"
        aria-label={t.curve + '. ' + t.chartHint}
        aria-valuemin="1"
        aria-valuemax={data.timeline.length}
        aria-valuenow={data.timeline.length}
      ></canvas>
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
</style>
