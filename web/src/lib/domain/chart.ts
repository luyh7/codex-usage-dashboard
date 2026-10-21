import type { Chart } from '../data/types';
import { compactNumber, chartPositions } from './presentation';
import { dateTime, type Language } from './ui';

export interface ChartPoint {
  x: number;
  y: number;
  tokens: number;
  time: string;
}
interface Options {
  data: Chart;
  lang: Language;
  zone: string;
  inspect: (point: ChartPoint | null) => void;
}

export function chart(canvas: HTMLCanvasElement, options: Options) {
  let current = options,
    positions: number[] = [];
  let width = 400,
    height = 210,
    max = 1,
    active = -1,
    frame = 0;
  let progress = 1,
    path = new Path2D(),
    area = new Path2D();
  const pad = { left: 44, right: 12, top: 14, bottom: 30 };
  const motion = matchMedia('(prefers-reduced-motion: reduce)');
  const x = (i: number) => pad.left + (width - pad.left - pad.right) * positions[i];
  const y = (i: number) =>
    height -
    pad.bottom -
    (height - pad.top - pad.bottom) *
      (current.data.timeline[i].total_token_usage.total_tokens / max);
  const clock = (index: number, timestamp?: number) => {
    const stamp =
      timestamp === undefined
        ? current.data.timeline[index]?.timestamp
        : new Date(timestamp).toISOString();
    if (!stamp || !Number.isFinite(Date.parse(stamp))) return String(index + 1);
    return new Intl.DateTimeFormat(current.lang === 'zh' ? 'zh-CN' : 'en-US', {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
      timeZone: current.zone,
    }).format(new Date(stamp));
  };
  function inspect() {
    const inspectedIndex = active >= 0 ? active : current.data.timeline.length - 1;
    const row = current.data.timeline[inspectedIndex];
    const announced = row || current.data.timeline.at(-1);
    canvas.setAttribute(
      'aria-valuenow',
      String(active >= 0 ? active + 1 : current.data.timeline.length),
    );
    canvas.setAttribute(
      'aria-valuetext',
      announced
        ? compactNumber(announced.total_token_usage.total_tokens) +
            ' tokens · ' +
            dateTime(announced.timestamp, current.zone, current.lang)
        : '—',
    );
    current.inspect(
      row
        ? {
            x: x(inspectedIndex),
            y: y(inspectedIndex),
            tokens: row.total_token_usage.total_tokens,
            time: row.timestamp ? clock(inspectedIndex) : '#' + (inspectedIndex + 1),
          }
        : null,
    );
  }
  function draw() {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const dpr = devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const style = getComputedStyle(canvas);
    const accent = style.getPropertyValue('--accent').trim();
    ctx.font = '13px ' + style.fontFamily;
    ctx.fillStyle = style.getPropertyValue('--muted').trim();
    ctx.strokeStyle = style.getPropertyValue('--border').trim();
    ctx.lineWidth = 1;
    for (let i = 0; i <= 3; i++) {
      const py = pad.top + ((height - pad.top - pad.bottom) * i) / 3;
      ctx.beginPath();
      ctx.moveTo(pad.left, py);
      ctx.lineTo(width - pad.right, py);
      ctx.stroke();
      ctx.textAlign = 'left';
      ctx.fillText(compactNumber(max * (1 - i / 3)), 0, py + 4);
    }
    const rows = current.data.timeline;
    if (!rows.length) return;
    const times = rows.map((row) => (row.timestamp ? Date.parse(row.timestamp) : NaN));
    const timed =
      times.at(-1)! > times[0] &&
      times.every((time, i) => Number.isFinite(time) && (!i || time >= times[i - 1]));
    const ticks = rows.length === 1 ? [0] : [0, 1 / 3, 2 / 3, 1];
    for (const fraction of ticks) {
      const index = Math.round((rows.length - 1) * fraction);
      const px = pad.left + (width - pad.left - pad.right) * fraction;
      const timestamp = timed ? times[0] + (times.at(-1)! - times[0]) * fraction : undefined;
      ctx.textAlign = fraction === 0 ? 'left' : fraction === 1 ? 'right' : 'center';
      ctx.fillText(clock(index, timestamp), px, height - 5);
      ctx.beginPath();
      ctx.moveTo(px, pad.top);
      ctx.lineTo(px, height - pad.bottom);
      ctx.stroke();
    }
    ctx.save();
    ctx.beginPath();
    ctx.rect(pad.left - 5, 0, (width - pad.left + 5) * progress, height - pad.bottom + 5);
    ctx.clip();
    const gradient = ctx.createLinearGradient(0, pad.top, 0, height - pad.bottom);
    gradient.addColorStop(0, style.getPropertyValue('--chart-fill').trim());
    gradient.addColorStop(1, 'transparent');
    ctx.fillStyle = gradient;
    ctx.fill(area);
    ctx.strokeStyle = accent;
    ctx.lineWidth = 2.4;
    ctx.lineJoin = 'round';
    ctx.stroke(path);
    // Mark a few real samples, retaining every event in the line and hit testing.
    for (const i of [
      ...new Set(
        Array.from({ length: Math.min(8, rows.length) }, (_, n) =>
          Math.round((n * (rows.length - 1)) / Math.max(1, Math.min(8, rows.length) - 1)),
        ),
      ),
    ]) {
      ctx.fillStyle = accent;
      ctx.beginPath();
      ctx.arc(x(i), y(i), 3, 0, Math.PI * 2);
      ctx.fill();
    }
    const point = active >= 0 ? active : rows.length - 1;
    if (active >= 0) {
      ctx.save();
      ctx.globalAlpha = 0.3;
      ctx.lineWidth = 1;
      ctx.setLineDash([3, 4]);
      ctx.beginPath();
      ctx.moveTo(x(point), pad.top);
      ctx.lineTo(x(point), height - pad.bottom);
      ctx.stroke();
      ctx.restore();
      ctx.globalAlpha = 0.15;
      ctx.fillStyle = accent;
      ctx.beginPath();
      ctx.arc(x(point), y(point), 10, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalAlpha = 1;
    }
    ctx.fillStyle = accent;
    ctx.strokeStyle = style.getPropertyValue('--panel').trim();
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(x(point), y(point), 4, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  }
  function prepare(animate = false) {
    cancelAnimationFrame(frame);
    width = canvas.clientWidth || 400;
    height = canvas.clientHeight || 210;
    const dpr = devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const rows = current.data.timeline;
    positions = chartPositions(rows);
    max = 1;
    for (const row of rows) max = Math.max(max, row.total_token_usage.total_tokens);
    const unit = 10 ** Math.floor(Math.log10(max / 3));
    const step = [1, 1.5, 2, 2.5, 5, 10].find((value) => value * unit >= max / 3)! * unit;
    max = step * 3;
    path = new Path2D();
    rows.forEach((_, i) => (i ? path.lineTo(x(i), y(i)) : path.moveTo(x(i), y(i))));
    area = new Path2D(path);
    if (rows.length) {
      area.lineTo(x(rows.length - 1), height - pad.bottom);
      area.lineTo(x(0), height - pad.bottom);
      area.closePath();
    }
    active = Math.min(active, rows.length - 1);
    progress = 1;
    inspect();
    if (animate && !motion.matches && rows.length > 1) {
      const start = performance.now();
      const step = (now: number) => {
        progress = Math.min(1, (now - start) / 650);
        draw();
        if (progress < 1) frame = requestAnimationFrame(step);
      };
      progress = 0;
      frame = requestAnimationFrame(step);
    } else draw();
  }
  function pointer(event: PointerEvent) {
    if (!positions.length) return;
    const px =
      (event.clientX - canvas.getBoundingClientRect().left - pad.left) /
      (width - pad.left - pad.right);
    let low = 0,
      high = positions.length - 1;
    while (low < high) {
      const mid = Math.floor((low + high) / 2);
      if (positions[mid] < px) low = mid + 1;
      else high = mid;
    }
    active =
      low > 0 && Math.abs(positions[low - 1] - px) < Math.abs(positions[low] - px) ? low - 1 : low;
    inspect();
    draw();
  }
  const leave = () => {
    active = -1;
    inspect();
    draw();
  };
  const focus = () => {
    active = current.data.timeline.length - 1;
    inspect();
    draw();
  };
  function keyboard(event: KeyboardEvent) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key) || !positions.length)
      return;
    event.preventDefault();
    active =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? positions.length - 1
          : Math.min(
              positions.length - 1,
              Math.max(0, active + (event.key === 'ArrowLeft' ? -1 : 1)),
            );
    inspect();
    draw();
  }
  const resize = new ResizeObserver(() => {
    if (canvas.clientWidth !== width || canvas.clientHeight !== height) prepare();
  });
  resize.observe(canvas);
  const reduce = () => prepare();
  motion.addEventListener('change', reduce);
  canvas.addEventListener('pointermove', pointer);
  canvas.addEventListener('pointerleave', leave);
  canvas.addEventListener('focus', focus);
  canvas.addEventListener('blur', leave);
  canvas.addEventListener('keydown', keyboard);
  prepare(true);
  return {
    update(next: Options) {
      const animate = current.data.id !== next.data.id;
      current = next;
      if (animate) active = -1;
      prepare(animate);
    },
    destroy() {
      cancelAnimationFrame(frame);
      resize.disconnect();
      motion.removeEventListener('change', reduce);
      canvas.removeEventListener('pointermove', pointer);
      canvas.removeEventListener('pointerleave', leave);
      canvas.removeEventListener('focus', focus);
      canvas.removeEventListener('blur', leave);
      canvas.removeEventListener('keydown', keyboard);
    },
  };
}
