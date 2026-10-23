/** Calendar arithmetic uses UTC day keys, independent of DST and the browser's zone. */
export const dayKey = (date: Date) => date.toISOString().slice(0, 10);

export function parseDay(value: string): Date | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return null;
  const date = new Date(value + 'T00:00:00Z');
  return Number.isFinite(+date) && dayKey(date) === value ? date : null;
}

export function todayInZone(zone: string, now = new Date()): string {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone: zone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).formatToParts(now);
  const part = (type: string) => parts.find((p) => p.type === type)!.value;
  return `${part('year')}-${part('month')}-${part('day')}`;
}

export function shiftDay(value: string, count: number): string {
  const date = parseDay(value)!;
  date.setUTCDate(date.getUTCDate() + count);
  return dayKey(date);
}

export function monthDays(year: number, month: number): string[] {
  const first = new Date(Date.UTC(year, month, 1));
  const offset = (first.getUTCDay() + 6) % 7;
  const count = new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
  const start = shiftDay(dayKey(first), -offset);
  return Array.from({ length: Math.ceil((offset + count) / 7) * 7 }, (_, i) => shiftDay(start, i));
}

export function rangeDays(from: string, to: string): number {
  const start = parseDay(from),
    end = parseDay(to);
  return start && end && end >= start ? Math.round((+end - +start) / 86400000) + 1 : 0;
}

/** Presets mirror the server: a week/month ends today, not in the future. */
export function presetRange(period: string, today: string, earliest = today) {
  const date = parseDay(today)!;
  const from =
    period === '7d'
      ? shiftDay(today, -6)
      : period === '30d'
        ? shiftDay(today, -29)
        : period === 'week'
          ? shiftDay(today, -((date.getUTCDay() + 6) % 7))
          : period === 'month'
            ? today.slice(0, 8) + '01'
            : period === 'all'
              ? earliest
              : today;
  return { from, to: today };
}
