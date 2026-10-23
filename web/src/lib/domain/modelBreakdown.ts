import type { ModelBreakdown } from '../data/types';

const colors = ['#159174', '#4b82bf', '#a080bd', '#cb9a44', '#5ba6a3', '#bd7288'];
const knownColors: Record<string, string> = {
  'gpt-6.1-sol': colors[0], 'gpt-6-astra': colors[1],
  'gpt-6-sol': colors[2], 'gpt-6-luna': colors[3],
  'gpt-5.6-sol': colors[4], 'gpt-5.6-luna': colors[5],
};
export function modelColor(model: string): string {
  let hash = 0;
  for (const char of model) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return model === 'unknown' ? '#9ca9a5' : knownColors[model.toLowerCase()] ?? colors[hash % colors.length];
}
export function modelRows(rows: ModelBreakdown[], metric: 'tokens' | 'cost') {
  const value = (row: ModelBreakdown) => metric === 'tokens'
    ? row.usage.total_tokens : (row.estimated_cost_usd ?? 0);
  const total = rows.reduce((sum, row) => sum + Math.max(0, value(row)), 0);
  return [...rows].sort((a, b) => value(b) - value(a) || a.model.localeCompare(b.model))
    .map(row => ({ ...row, color: modelColor(row.model), share: total ? Math.max(0, value(row)) / total : 0 }));
}
