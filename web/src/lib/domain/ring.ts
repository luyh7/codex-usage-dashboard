/** SVG arcs retain the dashboard's 17-unit radius and 44-unit viewBox. */
export function ringArc(percent: number, startPercent = 0): string {
  if (!Number.isFinite(percent) || !Number.isFinite(startPercent) || percent <= 0) return '';
  const span = Math.min(100, percent);
  const point = (value: number) => {
    const angle = value / 100 * Math.PI * 2;
    return [22 + 17 * Math.cos(angle), 22 + 17 * Math.sin(angle)]
      .map(coordinate => Number(coordinate.toFixed(4))).join(' ');
  };
  const start = point(startPercent);
  if (span === 100)
    return `M ${start} A 17 17 0 1 1 ${point(startPercent + 50)} A 17 17 0 1 1 ${start}`;
  return `M ${start} A 17 17 0 ${span > 50 ? 1 : 0} 1 ${point(startPercent + span)}`;
}
