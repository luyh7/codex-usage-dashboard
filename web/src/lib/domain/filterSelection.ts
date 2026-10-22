/** Multi-select query values use JSON arrays; single values retain existing links. */
export function filterValues(value: string): string[] {
  if (!value) return [];
  if (value.startsWith('[')) {
    try {
      const parsed: unknown = JSON.parse(value);
      if (Array.isArray(parsed) && parsed.every(item => typeof item === 'string'))
        return [...new Set(parsed.filter(Boolean))];
    } catch { /* Treat malformed arrays as a literal legacy value. */ }
  }
  return [value];
}

export function filterSelection(values: string[]): string {
  const selected = [...new Set(values.filter(Boolean))];
  return selected.length > 1 ? JSON.stringify(selected) : selected[0] || '';
}
