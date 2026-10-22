import type { Chart } from '../data/types';

export type Point = [number, number];

/** Monotone cubic controls preserve real peaks and flat intervals, including duplicate times. */
export function curveSegments(points: Point[]) {
  const slopes = points
    .slice(1)
    .map(([x, y], i) => (x > points[i][0] ? (y - points[i][1]) / (x - points[i][0]) : 0));
  const tangents = points.map((_, i) => {
    if (!i) return slopes[0] ?? 0;
    if (i === points.length - 1) return slopes[i - 1];
    const before = slopes[i - 1],
      after = slopes[i];
    return before * after <= 0 ? 0 : 2 / (1 / before + 1 / after);
  });
  slopes.forEach((slope, i) => {
    if (!slope) tangents[i] = tangents[i + 1] = 0;
    else {
      const length = Math.hypot(tangents[i] / slope, tangents[i + 1] / slope);
      if (length > 3) {
        tangents[i] *= 3 / length;
        tangents[i + 1] *= 3 / length;
      }
    }
  });
  return points.slice(1).map(([x, y], i) => {
    const [previousX, previousY] = points[i],
      step = (x - previousX) / 3;
    return [
      previousX + step,
      previousY + step * tangents[i],
      x - step,
      y - step * tangents[i + 1],
      x,
      y,
    ] as const;
  });
}

/** Group events only when their timestamps cannot be distinguished on screen. */
export function usageBars(timeline: Chart['timeline'], positions: number[], width: number) {
  const count = Math.max(1, Math.floor(width / 10));
  const bars: { column: number; start: number; end: number; tokens: number }[] = [];
  timeline.forEach((row, i) => {
    const column = Math.min(count - 1, Math.floor(positions[i] * count));
    const tokens = Math.max(
      0,
      row.total_token_usage.total_tokens - (timeline[i - 1]?.total_token_usage.total_tokens ?? 0),
    );
    const previous = bars.at(-1);
    if (previous?.column === column) {
      previous.end = i;
      previous.tokens += tokens;
    } else bars.push({ column, start: i, end: i, tokens });
  });
  return { bars, count };
}

export function chartDates(zone: string, timestamps: (string | null)[], now = Date.now()) {
  const formatter = new Intl.DateTimeFormat('en-GB', {
    timeZone: zone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  });
  const parts = (stamp: string | number | null) => {
    if (stamp === null || !Number.isFinite(new Date(stamp).getTime())) return null;
    const values = Object.fromEntries(
      formatter.formatToParts(new Date(stamp)).map((part) => [part.type, part.value]),
    );
    return {
      day: `${values.year}-${values.month}-${values.day}`,
      year: values.year,
      date: `${values.month}/${values.day}`,
      time: `${values.hour}:${values.minute}`,
    };
  };
  const today = parts(now)!;
  const dates = timestamps.map(parts);
  const showYear = dates.some((date) => date && date.year !== today.year);
  const dated = (date: NonNullable<ReturnType<typeof parts>>) =>
    `${showYear ? date.year + '/' : ''}${date.date}`;
  return {
    tooltip: (stamp: string | null) => {
      const date = parts(stamp);
      return date ? `${dated(date)} ${date.time}` : null;
    },
    ticks: (stamps: (string | null)[]) => {
      const ticks = stamps.map(parts);
      const valid = stamps
        .map((stamp) => (stamp === null ? NaN : Date.parse(stamp)))
        .filter(Number.isFinite);
      const daysOnly = valid.length > 1 && Math.max(...valid) - Math.min(...valid) >= 2 * 86400000;
      const crossesDate =
        new Set(dates.filter((date) => date !== null).map((date) => date.day)).size > 1;
      return ticks.map((date, i) => {
        if (!date) return { time: `#${i + 1}`, date: '' };
        const changed = i > 0 && date.day !== ticks[i - 1]?.day;
        const showDate = changed || (!i && (date.day !== today.day || crossesDate));
        return daysOnly
          ? { time: '', date: !i || changed ? dated(date) : '' }
          : { time: date.time, date: showDate ? dated(date) : '' };
      });
    },
  };
}
