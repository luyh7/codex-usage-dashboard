<script lang="ts">
  import type { Language } from '../lib/domain/ui';
  import { number } from '../lib/domain/ui';
  import { compactNumber } from '../lib/domain/presentation';
  import { metricTooltip } from '../lib/domain/metricTooltip';
  import { metricIntervalLabel } from '../lib/domain/metrics';
  let {
    values,
    label,
    lang,
  }: {
    values: { date: string; end: string; value: number }[];
    label: string;
    lang: Language;
  } = $props();
  let max = $derived(Math.max(1, ...values.map((row) => row.value)));
</script>

{#if values.length}
  <div class="metric-spark daily-usage" role="group" aria-label={label}>
    <div
      class="daily-columns"
      style:grid-template-columns={`repeat(${values.length}, minmax(0, 1fr))`}
    >
      {#each values as row, i (`${row.date}/${row.end}`)}
        {@const date = metricIntervalLabel(row.date, row.end)}
        <button
          type="button"
          class="daily-column"
          onpointerdown={(event) => {
            event.preventDefault();
            event.currentTarget.blur();
          }}
          aria-label={`${date}: ${number(row.value, lang)} tokens`}
          use:metricTooltip={{
            title: date,
            rows: [{ label, value: `${compactNumber(row.value)} tokens` }],
          }}
        >
          <span
            class="daily-bar"
            class:latest={i === values.length - 1}
            style:height={`${(row.value / max) * 100}%`}
            data-date={row.date}
            data-end={row.end}
            data-value={row.value}
          ></span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .daily-usage {
    position: absolute;
    left: 24px;
    right: 24px;
    bottom: var(--metric-chart-bottom);
    width: auto;
    height: 40px;
  }
  .daily-columns {
    display: grid;
    height: 100%;
  }
  .daily-column {
    display: flex;
    align-items: flex-end;
    justify-content: center;
    height: 100%;
    min-width: 0;
    min-height: 0;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    cursor: default;
  }
  .daily-column:hover,
  .daily-column:focus-visible {
    background: transparent;
  }
  .daily-column:active {
    transform: none;
  }
  .daily-bar {
    width: min(22px, 60%);
    background: #a0dfc9;
    border-radius: 3px;
    transform-origin: bottom;
    transition: height 450ms ease-out, transform 180ms ease-out, background 180ms ease-out,
      box-shadow 180ms ease-out;
  }
  .daily-bar.latest {
    background: var(--accent);
  }
  .daily-column:hover .daily-bar,
  .daily-column:focus-visible .daily-bar {
    background: var(--accent);
    transform: scaleY(1.1);
    box-shadow: 0 0 0 3px #00a46918;
  }
  @media (prefers-reduced-motion: reduce) {
    .daily-bar { transition: none; }
    .daily-column:hover .daily-bar,
    .daily-column:focus-visible .daily-bar { transform: none; }
  }
  @media (max-width: 1400px) and (min-width: 1101px) {
    .daily-usage {
      left: 18px;
      right: 18px;
    }
  }
  @media (max-width: 1100px), (max-height: 650px) {
    .daily-usage {
      left: 18px;
      right: 18px;
      height: 22px;
    }
  }
  @media (max-width: 760px) {
    .daily-usage {
      display: none;
    }
  }
</style>
