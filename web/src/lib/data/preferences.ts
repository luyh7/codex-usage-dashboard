import { writeHeaders } from '../../../shared/write-token.js';
import type { SourcePolling } from '../domain/sourcePolling';
export type Preferences = { schema_version: number; language: 'zh' | 'en' | null; timezone: string | null; codex_homes: string[]; source_polling?: Record<string, SourcePolling> };
let queue: Promise<unknown> = Promise.resolve();
export async function preferences(): Promise<Preferences> {
  const response = await fetch('/api/settings', { cache: 'no-store' });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || 'Could not read settings');
  return value;
}
export function savePreferences(patch: Partial<Preferences>): Promise<Preferences> {
  const next = queue.catch(() => {}).then(async () => {
    const value = { ...patch, schema_version: 1 };
    const response = await fetch('/api/settings', { method: 'POST', headers: { 'Content-Type': 'application/json', ...await writeHeaders('') }, body: JSON.stringify(value) });
    const saved = await response.json();
    if (!response.ok) throw new Error(saved.error || 'Could not save settings');
    return saved as Preferences;
  });
  queue = next;
  return next;
}
