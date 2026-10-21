import type { Detail, Chart } from '../data/types';

/** Short values retain the full precision in the surrounding element's title. */
export function compactNumber(value: number | null | undefined): string {
  const n = value ?? 0;
  for (const [scale, unit] of [
    [1e9, 'B'],
    [1e6, 'M'],
    [1e3, 'K'],
  ] as const) {
    if (Math.abs(n) >= scale)
      return `${(n / scale).toLocaleString('en-US', { minimumFractionDigits: scale >= 1e6 ? 2 : 0, maximumFractionDigits: 2 })}${unit}`;
  }
  return n.toLocaleString('en-US', { maximumFractionDigits: 1 });
}

export function displayMoney(value: number): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: value !== 0 && Math.abs(value) < 0.01 ? 6 : 2,
  }).format(value);
}

/** The backend's output cost already includes reasoning. Never add it twice. */
export function costSegments(detail: Pick<Detail, 'estimated_cost_breakdown_usd'>) {
  const parts = detail.estimated_cost_breakdown_usd;
  if (!parts) return [];
  return [
    { key: 'input' as const, value: parts.input_tokens ?? 0, color: 'var(--cost-input)' },
    { key: 'cached' as const, value: parts.cached_input_tokens ?? 0, color: 'var(--cost-cache)' },
    { key: 'output' as const, value: parts.output_tokens ?? 0, color: 'var(--cost-output)' },
    ...(parts.cache_write_tokens
      ? [
          {
            key: 'cacheWrite' as const,
            value: parts.cache_write_tokens,
            color: 'var(--cost-write)',
          },
        ]
      : []),
  ];
}

/** Use elapsed time when timestamps allow it; imported undated events use order. */
export function chartPositions(timeline: Chart['timeline']): number[] {
  const times = timeline.map((row) => (row.timestamp ? Date.parse(row.timestamp) : NaN));
  const start = times[0],
    end = times.at(-1) ?? NaN;
  const timed =
    end > start && times.every((time, i) => Number.isFinite(time) && (!i || time >= times[i - 1]));
  return times.map((time, i) =>
    timed ? (time - start) / (end - start) : i / Math.max(1, times.length - 1),
  );
}
