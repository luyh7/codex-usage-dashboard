import type { Chart } from '../data/types';
export function chart(canvas: HTMLCanvasElement, data: Chart) {
  let current = data;
  function draw() {
    const width = canvas.clientWidth || 400,
      height = 170,
      dpr = devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);
    const rows = current.timeline;
    if (!rows.length) return;
    let max = 1;
    for (const row of rows) max = Math.max(max, row.total_token_usage.total_tokens);
    ctx.strokeStyle = getComputedStyle(canvas).color;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    rows.forEach((row, i) => {
      const x = 8 + ((width - 16) * i) / Math.max(1, rows.length - 1),
        y = height - 8 - ((height - 16) * row.total_token_usage.total_tokens) / max;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  }
  const observer = new ResizeObserver(draw);
  observer.observe(canvas);
  draw();
  return {
    update(data: Chart) {
      current = data;
      draw();
    },
    destroy() {
      observer.disconnect();
    },
  };
}
