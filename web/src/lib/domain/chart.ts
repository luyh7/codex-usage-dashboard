import type { Chart } from '../data/types';
import { compactNumber, chartPositions } from './presentation';
import { dateTime, words, type Language } from './ui';
import { chartDates, curveSegments, usageBars } from './chartModel';
import { openTooltip } from './tooltip';

interface Options {
  data: Chart;
  lang: Language;
  zone: string;
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
  let alive = true;
  const pad = { left: 54, right: 8, top: 14, bottom: 30 };
  let bars: ReturnType<typeof usageBars> = { bars: [], count: 1 };
  let ticks: { x: number; time: string; date: string }[] = [];
  let dates = chartDates(current.zone, []);
  let tooltip: ReturnType<typeof openTooltip> | null = null;
  let pointerPosition: { x: number; y: number } | null = null;
  const motion = matchMedia('(prefers-reduced-motion: reduce)');
  const x = (i: number) => pad.left + (width - pad.left - pad.right) * positions[i];
  const y = (i: number) =>
    height -
    pad.bottom -
    (height - pad.top - pad.bottom) *
      (current.data.timeline[i].total_token_usage.total_tokens / max);
  function inspect() {
    const inspectedIndex = active;
    const row = current.data.timeline[inspectedIndex];
    const announced = row || current.data.timeline.at(-1);
    const bar = bars.bars.find((bar) => active >= bar.start && active <= bar.end);
    canvas.setAttribute(
      'aria-valuenow',
      String(active >= 0 ? active + 1 : current.data.timeline.length),
    );
    canvas.setAttribute(
      'aria-valuetext',
      announced
        ? compactNumber(announced.total_token_usage.total_tokens) +
            ' tokens · ' +
            dateTime(announced.timestamp, current.zone, current.lang) +
            (bar
              ? ` · ${words(current.lang).intervalTokens} ${compactNumber(bar.tokens)} tokens`
              : '')
        : '—',
    );
    if (!row) { tooltip?.close(); return; }
    const content = document.createDocumentFragment();
    const total = document.createElement('strong');
    total.textContent = compactNumber(row.total_token_usage.total_tokens) + ' tokens';
    const time = document.createElement('span');
    time.textContent = dates.tooltip(row.timestamp) ?? '#' + (inspectedIndex + 1);
    const interval = document.createElement('span');
    interval.append(words(current.lang).intervalTokens + ' ');
    const value = document.createElement('b');
    value.textContent = compactNumber(bar?.tokens ?? 0);
    interval.append(value);
    content.append(total, time, interval);
    const rect = canvas.getBoundingClientRect();
    const position = pointerPosition ?? { x: rect.left + x(inspectedIndex), y: rect.top + y(inspectedIndex) };
    if (tooltip) {
      tooltip.update(content);
      tooltip.place(position.x, position.y);
    } else {
      tooltip = openTooltip(canvas, content, {
        className: 'chart-tooltip', side: 'pointer', ...position,
        onClose: () => { tooltip = null; active = -1; pointerPosition = null; if (alive) draw(); },
      });
    }
  }
  function draw() {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const dpr = devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const style = getComputedStyle(canvas);
    const accent = style.getPropertyValue('--accent').trim();
    ctx.font = '12px ' + style.fontFamily;
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
      ctx.fillText(compactNumber(max * (1 - i / 3)).replace(/(\.\d)0(?=[KMB]$)/, '$1'), 0, py + 4);
    }
    const rows = current.data.timeline;
    if (!rows.length) return;
    for (const [i, tick] of ticks.entries()) {
      const px = tick.x;
      ctx.textAlign = i === 0 ? 'left' : i === ticks.length - 1 ? 'right' : 'center';
      if (tick.time) ctx.fillText(tick.time, px, height - pad.bottom + 18);
      if (tick.date) ctx.fillText(tick.date, px, height - pad.bottom + (tick.time ? 34 : 18));
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
    // Keep the same mint hue at both ends; transparent black makes the area gray.
    gradient.addColorStop(1, '#33bc8b00');
    ctx.fillStyle = gradient;
    ctx.fill(area);
    // Bars use the same token axis as the curve, retaining the actual relative magnitudes.
    const barStep = (width - pad.left - pad.right) / bars.count;
    for (const bar of bars.bars) {
      if (!bar.tokens) continue;
      ctx.fillStyle = accent;
      ctx.globalAlpha = active >= bar.start && active <= bar.end ? 0.5 : 0.23;
      const barHeight = ((height - pad.top - pad.bottom) * bar.tokens) / max;
      ctx.fillRect(
        pad.left + bar.column * barStep + 1,
        height - pad.bottom - barHeight,
        Math.max(1, barStep - 2),
        barHeight,
      );
    }
    ctx.globalAlpha = 1;
    ctx.strokeStyle = accent;
    ctx.lineWidth = 2.4;
    ctx.lineJoin = 'round';
    ctx.lineCap = 'round';
    ctx.stroke(path);
    const point = active;
    if (active >= 0) {
      ctx.save();
      ctx.strokeStyle = accent;
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
      ctx.fillStyle = accent;
      ctx.strokeStyle = style.getPropertyValue('--panel').trim();
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(x(point), y(point), 5.5, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
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
    dates = chartDates(
      current.zone,
      rows.map((row) => row.timestamp),
    );
    const times = rows.map((row) => (row.timestamp ? Date.parse(row.timestamp) : NaN));
    const timed =
      times.at(-1)! > times[0] &&
      times.every((time, i) => Number.isFinite(time) && (!i || time >= times[i - 1]));
    const plotWidth = width - pad.left - pad.right;
    const count = Math.min(rows.length, plotWidth < 240 ? 2 : plotWidth < 400 ? 3 : 4);
    const fractions = Array.from({ length: count }, (_, i) => i / Math.max(1, count - 1));
    const stamps = fractions.map((fraction) =>
      timed
        ? new Date(times[0] + (times.at(-1)! - times[0]) * fraction).toISOString()
        : rows[Math.round((rows.length - 1) * fraction)].timestamp,
    );
    ticks = dates
      .ticks(stamps)
      .map((tick, i) => ({ ...tick, x: pad.left + (width - pad.left - pad.right) * fractions[i] }));
    pad.bottom = ticks.some((tick) => tick.date && tick.time) ? 46 : 30;
    bars = usageBars(rows, positions, width - pad.left - pad.right);
    max = 1;
    for (const row of rows) max = Math.max(max, row.total_token_usage.total_tokens);
    for (const bar of bars.bars) max = Math.max(max, bar.tokens);
    const unit = 10 ** Math.floor(Math.log10(max / 3));
    const step = [1, 1.5, 2, 2.5, 5, 10].find((value) => value * unit >= max / 3)! * unit;
    max = step * 3;
    path = new Path2D();
    if (rows.length) path.moveTo(x(0), y(0));
    for (const controls of curveSegments(rows.map((_, i) => [x(i), y(i)])))
      path.bezierCurveTo(...controls);
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
    if (!positions.length || event.pointerType === 'touch') return;
    const rect = canvas.getBoundingClientRect();
    const localX = event.clientX - rect.left,
      localY = event.clientY - rect.top;
    if (
      localX < pad.left ||
      localX > width - pad.right ||
      localY < pad.top ||
      localY > height - pad.bottom
    ) {
      leave();
      return;
    }
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
    pointerPosition = { x: event.clientX, y: event.clientY };
    inspect();
    draw();
  }
  const leave = () => {
    pointerPosition = null;
    active = -1;
    inspect();
    draw();
  };
  const focus = () => {
    if (!canvas.matches(':focus-visible')) return;
    pointerPosition = null;
    active = current.data.timeline.length - 1;
    inspect();
    draw();
  };
  function keyboard(event: KeyboardEvent) {
    if (event.key === 'Escape' && active >= 0) {
      event.preventDefault();
      event.stopPropagation();
      leave();
      return;
    }
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key) || !positions.length)
      return;
    event.preventDefault();
    pointerPosition = null;
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
  // Canvas text does not repaint automatically after the bundled fonts load.
  void document.fonts.ready.then(() => {
    if (alive) draw();
  });
  return {
    update(next: Options) {
      const animate = current.data.id !== next.data.id;
      current = next;
      if (animate) active = -1;
      prepare(animate);
    },
    destroy() {
      alive = false;
      tooltip?.close();
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
