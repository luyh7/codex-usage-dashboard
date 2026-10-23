<script lang="ts">
  import { metricTooltip } from '../lib/domain/metricTooltip';
  import { metricAxisLabel, metricIntervalLabel } from '../lib/domain/metrics';
  let {
    values,
    label,
    intervalLabel,
    startLabel,
    format,
  }: {
    values: { date: string; end: string; value: number | null }[];
    label: string;
    intervalLabel: string;
    startLabel: string;
    format: (value: number) => string;
  } = $props();
  let width = $state(220);
  let height = $state(94);
  const gradientId = $props.id();
  let chartWidth = $derived(Math.max(16, width));
  let chartHeight = $derived(Math.max(16, height));
  let zeroY = $derived(chartHeight - 1.2);
  let cumulative = $derived.by(() => {
    let total = 0;
    return [0, ...values.map((row) => (total += row.value ?? 0))];
  });
  let max = $derived(Math.max(1e-9, ...cumulative));
  let points = $derived(
    cumulative.map((value, i) => [
      8 + (i * (chartWidth - 16)) / Math.max(1, cumulative.length - 1),
      zeroY - (value / max) * (zeroY - 8),
    ]),
  );
  // Monotone cubic interpolation rounds the joins without inventing cost peaks or dips.
  let line = $derived.by(() => {
    const slopes = points.slice(1).map(([x, y], i) => (y - points[i][1]) / (x - points[i][0]));
    const tangents = points.map((_, i) => {
      if (i === 0) return slopes[0] ?? 0;
      if (i === points.length - 1) return slopes[i - 1];
      const before = slopes[i - 1];
      const after = slopes[i];
      return before * after <= 0 ? 0 : (2 * before * after) / (before + after);
    });
    return points
      .map(([x, y], i) => {
        if (i === 0) return `M${x},${y}`;
        const [previousX, previousY] = points[i - 1];
        const step = (x - previousX) / 3;
        return `C${previousX + step},${previousY + step * tangents[i - 1]} ${x - step},${y - step * tangents[i]} ${x},${y}`;
      })
      .join(' ');
  });
  let known = $derived(values.every((row) => row.value !== null));
</script>

{#if values.length && known}
  <div
    class="metric-spark line-spark"
    role="group"
    aria-label={label}
    bind:clientWidth={width}
    bind:clientHeight={height}
  >
    <svg viewBox={`0 0 ${chartWidth} ${chartHeight}`} aria-label={label}>
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="#00a469" stop-opacity=".24" />
          <stop offset="100%" stop-color="#00a469" stop-opacity="0" />
        </linearGradient>
      </defs>
      <!-- Reveal on mount only; background updates keep the curve and inspection targets. -->
      <g class="cost-reveal">
        <path
          d={`${line} L${chartWidth - 8},${zeroY} L8,${zeroY} Z`}
          fill={`url(#${gradientId})`}
        />
        <path
          d={line}
          fill="none"
          stroke="#00a469"
          stroke-width="2.4"
          stroke-linecap="round"
          stroke-linejoin="round"
          vector-effect="non-scaling-stroke"
        />
      </g>
      {#each points as [x, y], i (i === 0 ? `start:${values[0].date}` : `${values[i - 1].date}/${values[i - 1].end}`)}
        {@const row = values[Math.max(0, i - 1)]}
        {@const date =
          i === 0
            ? `${row.date.includes('T') ? `${row.date.slice(0, 10)} ${metricAxisLabel(row.date)}` : row.date} · ${startLabel}`
            : metricIntervalLabel(row.date, row.end)}
        {@const rows = [
          { label, value: format(cumulative[i]) },
          ...(i ? [{ label: intervalLabel, value: format(row.value ?? 0) }] : []),
        ]}
        <g class="cost-point">
          <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users inspect chart values on focus.) -->
          <rect
            x={i ? (points[i - 1][0] + x) / 2 : 0}
            y="0"
            width={(i === points.length - 1 ? chartWidth : (points[i + 1][0] + x) / 2) -
              (i ? (points[i - 1][0] + x) / 2 : 0)}
            height={chartHeight}
            fill="transparent"
            tabindex="0"
            role="img"
            onpointerdown={(event) => {
              event.preventDefault();
              event.currentTarget.blur();
            }}
            aria-label={`${date}: ${label} ${format(cumulative[i])}`}
            use:metricTooltip={{ title: date, rows, followPointer: true }}
            data-cost={cumulative[i]}
            data-date={i ? row.end : row.date}
          />
          <g class="cost-marker" aria-hidden="true">
            <circle class="cost-halo" cx={x} cy={y} r="8" />
            <circle class="cost-dot" cx={x} cy={y} r="3.5" />
          </g>
        </g>
      {/each}
    </svg>
  </div>
{/if}

<style>
  .line-spark svg {
    display: block;
    width: 100%;
    height: 100%;
  }
  .cost-point rect:focus {
    outline: none;
  }
  .cost-point rect:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -2px;
  }
  .cost-reveal {
    animation: cost-reveal 650ms linear both;
  }
  @keyframes cost-reveal {
    from {
      clip-path: inset(0 100% 0 0);
    }
    to {
      clip-path: inset(0 0 0 0);
    }
  }
  .cost-marker {
    opacity: 0;
    pointer-events: none;
    transition: opacity 160ms ease-out;
  }
  .cost-halo {
    fill: var(--accent);
    fill-opacity: 0.15;
  }
  .cost-dot {
    fill: var(--accent);
    stroke: var(--panel);
    stroke-width: 1.5;
  }
  .cost-point:hover .cost-marker,
  .cost-point:focus-within .cost-marker {
    opacity: 1;
  }
  @media (prefers-reduced-motion: reduce) {
    .cost-reveal {
      animation: none;
    }
    .cost-marker {
      transition: none;
    }
  }
</style>
