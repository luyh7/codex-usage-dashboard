<script lang="ts">
  import type { Chart } from '../lib/data/types';
  import { chart, type ChartPoint } from '../lib/domain/chart';
  import { compactNumber } from '../lib/domain/presentation';
  import { words, number, type Language } from '../lib/domain/ui';
  let { data, lang, zone }: { data: Chart; lang: Language; zone: string } = $props();
  let point = $state<ChartPoint | null>(null);
  let t = $derived(words(lang));
  const inspect = (next: ChartPoint | null) => {
    point = next;
  };
</script>

<figure class="usage-chart">
  <figcaption>
    <h3>{t.usageTrend}</h3>
    <span class="chart-legend"><span class="tiny-dot"></span>{t.cumulative}</span>
  </figcaption>
  {#if data.timeline.length}
    <div class="chart-surface">
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
          style:left={`clamp(0px, ${point.x - 125}px, calc(100% - 132px))`}
          style:top={Math.max(-26, point.y - 58) + 'px'}
        >
          <strong title={number(point.tokens, lang)}
            >{compactNumber(point.tokens)} <small>tokens</small></strong
          ><span>{point.time}</span>
        </div>{/if}
    </div>
    <p class="sr-only">{t.chartHint}</p>
  {:else}<p class="chart-empty">{t.noEvents}</p>{/if}
</figure>
