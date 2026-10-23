import type { Detail, Chart } from '../data/types';
import type { Session, Usage } from '../data/types';
import type { Language } from './ui';

/** Group cache rates use summed input tokens, never an average of session percentages. */
export function cacheHitPercent(usage: Pick<Usage, 'input_tokens' | 'cached_input_tokens'>) {
  if (usage.input_tokens <= 0) return null;
  const percent = usage.cached_input_tokens / usage.input_tokens * 100;
  return Number.isFinite(percent) ? Math.round(Math.min(100, Math.max(0, percent)) * 10) / 10 : null;
}

/** Shared bands keep the overview chart and every cache column consistent. */
export function cacheHitTone(percent: number | null) {
  if (percent === null || !Number.isFinite(percent)) return 'empty';
  if (percent < 50) return 'low';
  if (percent < 80) return 'medium';
  if (percent < 95) return 'high';
  return 'excellent';
}

export function serviceTier(value: string | null | undefined) {
  const tier = value?.trim().toLowerCase() || 'standard';
  return tier === 'priority' ? 'fast' : tier;
}

export function serviceTierLabel(value: string | null | undefined) {
  const tier = serviceTier(value);
  return tier === 'fast' ? 'Fast' : tier === 'ultrafast' ? 'Ultrafast' : tier;
}

export function sessionMetadata(sessions: (Pick<Session, 'models' | 'model' | 'effort'> & Partial<Pick<Session, 'service_tier'>>)[]) {
  const unique = (values: string[]) => [...new Set(values.map(value => value.trim()).filter(Boolean))];
  return {
    models: unique(sessions.flatMap(row => row.models?.length ? row.models : [row.model || ''])),
    efforts: unique(sessions.map(row => row.effort || '')),
    speedTiers: unique(sessions.map(row => serviceTier(row.service_tier)))
      .filter(tier => tier === 'fast' || tier === 'ultrafast'),
  };
}

/** Match the server's latest positive token delta; log/settings updates do not count. */
export function activityTime(
  row: Pick<Session, 'last_usage_at'>,
) {
  return row.last_usage_at;
}

export function latestActivityTime(rows: Pick<Session, 'last_usage_at'>[]) {
  return rows.map(activityTime).filter((time): time is string => !!time && Number.isFinite(Date.parse(time)))
    .sort((a, b) => Date.parse(b) - Date.parse(a))[0];
}

export function relativeTime(value: string | null | undefined, lang: Language, now = Date.now()) {
  if (!value) return '—';
  const time = Date.parse(value);
  if (!Number.isFinite(time)) return '—';
  const seconds = Math.max(0, Math.floor((now - time) / 1000));
  if (seconds < 60) return lang === 'zh' ? '刚刚' : 'Just now';
  const [scale, zh, en] =
    seconds < 3600
      ? [60, '分钟', 'min']
      : seconds < 172800
        ? [3600, '小时', 'h']
        : seconds < 2592000
          ? [86400, '天', 'd']
          : seconds < 31536000
            ? [2592000, '个月', 'mo']
            : [31536000, '年', 'y'];
  return `${Math.floor(seconds / Number(scale))} ${lang === 'zh' ? zh : en}`;
}

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
