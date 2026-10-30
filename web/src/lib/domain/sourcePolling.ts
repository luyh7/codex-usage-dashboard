export type SourcePolling = { poll_interval_secs?: number };
export type SourceRow = {
  path: string;
  status: string;
  network?: boolean;
  poll_interval_secs?: number;
};
export const MIN_NETWORK_POLL_SECS = 10;
export const DEFAULT_NETWORK_POLL_SECS = 30;
export const MAX_POLL_SECS = 86_400;

/** Mirrors the server: network sources never poll faster than 10 s. */
export function clampPollInterval(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_NETWORK_POLL_SECS;
  return Math.min(MAX_POLL_SECS, Math.max(MIN_NETWORK_POLL_SECS, Math.round(value)));
}

/** Next `source_polling` map with one source's interval replaced; other sources are kept. */
export function withSourcePolling(
  current: Record<string, SourcePolling>,
  path: string,
  change: SourcePolling,
): Record<string, SourcePolling> {
  const merged: SourcePolling = { ...current[path], ...change };
  const next = { ...current };
  if (merged.poll_interval_secs !== undefined) next[path] = { poll_interval_secs: clampPollInterval(merged.poll_interval_secs) };
  else delete next[path];
  return next;
}

/** Self-explanatory chip text for a network source, e.g. "网络盘 · 每 30 秒检查". */
export function networkPollingLabel(seconds: number | undefined, lang: 'zh' | 'en'): string {
  const value = seconds ?? DEFAULT_NETWORK_POLL_SECS;
  return lang === 'zh' ? `网络盘 · 每 ${value} 秒检查` : `Network drive · checks every ${value} s`;
}
