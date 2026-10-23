import type { MetricTrend, Period } from '../data/types';
import { parseDay, rangeDays, shiftDay } from './calendar';

export interface MetricBucket {
  date: string;
  end: string;
  tokens: number;
  sessions: number;
  cost: number | null;
}

/** Bound visual density while retaining every event, zero day and unknown price. */
export function metricSeries(
  trend: MetricTrend | undefined,
  period: Period | null,
): MetricBucket[] {
  if (!trend?.points.length) return [];
  if (trend.granularity === 'hour') {
    return trend.points.map((point) => ({
      date: point.date,
      end: point.end ?? point.date,
      tokens: point.usage.total_tokens,
      sessions: point.session_count,
      cost: point.estimated_cost_usd,
    }));
  }
  const valid = trend.points.filter((point) => parseDay(point.date));
  if (!valid.length) return [];
  const start =
    period?.start_date || valid.reduce((a, b) => (a < b.date ? a : b.date), valid[0].date);
  const end = period?.end_date || valid.reduce((a, b) => (a > b.date ? a : b.date), valid[0].date);
  const count = rangeDays(start, end);
  if (!count) return [];
  const size = Math.ceil(count / 28);
  const buckets: MetricBucket[] = Array.from({ length: Math.ceil(count / size) }, (_, i) => ({
    date: shiftDay(start, i * size),
    end: shiftDay(start, Math.min(count - 1, (i + 1) * size - 1)),
    tokens: 0,
    sessions: 0,
    cost: 0,
  }));
  for (const point of valid) {
    if (point.date < start || point.date > end) continue;
    const bucket = buckets[Math.floor((rangeDays(start, point.date) - 1) / size)];
    bucket.tokens += point.usage.total_tokens;
    bucket.sessions += point.session_count;
    bucket.cost =
      bucket.cost === null || point.estimated_cost_usd === null
        ? null
        : bucket.cost + point.estimated_cost_usd;
  }
  return buckets;
}

export const metricAxisLabel = (date: string) =>
  date.includes('T') ? date.slice(11, 16) : date.slice(5).replace('-', '/');

export function metricIntervalLabel(date: string, end: string): string {
  if (date.includes('T')) {
    const stop = end.slice(0, 10) === date.slice(0, 10) ? end.slice(11, 16) : '24:00';
    return `${date.slice(0, 10)} ${date.slice(11, 16)} – ${stop}`;
  }
  return date + (end !== date ? ` – ${end}` : '');
}
