import { writeHeaders } from './write-token.js';
/**
 * @param {string} url
 * @param {(percent: number, phase: string) => void} onProgress
 * @param {{fetcher?: typeof fetch, pause?: (ms: number) => Promise<void>, save?: (blob: Blob, filename: string) => void}} options
 */
export async function downloadSnapshot(url, onProgress, options = {}) {
  const fetcher = options.fetcher || fetch;
  const pause = options.pause || (ms => new Promise(resolve => setTimeout(resolve, ms)));
  const save = options.save || saveDownload;
  const base = url.replace(/\/api\/export\.json(?:\.gz)?(?:\?.*)?$/, '');
  const redacted = new URLSearchParams(url.split('?')[1] || '').get('mode') === 'redacted';
  let percent = 0;
  const report = (value, phase) => { percent = Math.max(percent, Math.min(100, value)); onProgress(percent, phase); };
  async function request(path, init = {}) {
    const response = await fetcher(base + path, { cache: 'no-store', ...init });
    if (!response.ok) {
      const body = await response.json().catch(() => ({}));
      throw new Error(body.error || `HTTP ${response.status}`);
    }
    return response;
  }
  report(0, 'preparing');
  let job = await (await request('/api/export/start' + (redacted ? '?mode=redacted' : ''), {
    method: 'POST', headers: { 'Content-Type': 'application/json', ...await writeHeaders(base, fetcher) }, body: '{}',
  })).json();
  while (true) {
    if (job.phase === 'error') throw new Error(job.error || 'Export failed');
    report(job.percent, job.phase === 'ready' ? 'downloading' : job.phase);
    if (job.phase === 'ready') break;
    await pause(150);
    job = await (await request('/api/export/progress?id=' + encodeURIComponent(job.id))).json();
  }
  const response = await request('/api/export.json.gz?job=' + encodeURIComponent(job.id) + (redacted ? '&mode=redacted' : ''));
  const disposition = response.headers.get('content-disposition') || '';
  let filename = disposition.match(/filename="([^"]+)"/i)?.[1] || (redacted ? 'cud-shared.json.gz' : 'cud-snapshot.json.gz');
  const encoded = disposition.match(/filename\*=UTF-8''([^;]+)/i)?.[1];
  if (encoded) {
    try { filename = decodeURIComponent(encoded); } catch { /* Keep the compatible filename. */ }
  }
  const total = Number(response.headers.get('content-length'));
  const chunks = [];
  let received = 0;
  if (response.body) {
    const reader = response.body.getReader();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      received += value.byteLength;
      if (total > 0) report(95 + Math.min(4, Math.floor(4 * received / total)), 'downloading');
    }
  } else { chunks.push(await response.arrayBuffer()); }
  if (response.body && total > 0 && received !== total) throw new Error('Incomplete snapshot download');
  save(new Blob(chunks, { type: 'application/gzip' }), filename);
  report(100, 'complete');
}
function saveDownload(blob, filename) {
  const objectUrl = URL.createObjectURL(blob), link = document.createElement('a');
  link.href = objectUrl; link.download = filename; document.body.append(link); link.click(); link.remove();
  setTimeout(() => URL.revokeObjectURL(objectUrl), 60000);
}
