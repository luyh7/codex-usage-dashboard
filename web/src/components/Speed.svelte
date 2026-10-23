<script lang="ts">
  import type { SpeedTotals } from '../lib/data/types';
  import type { Language } from '../lib/domain/ui';
  import { formatTps, speedHint } from '../lib/domain/speed';
  let { value, lang, compact = false }: { value?: SpeedTotals | null; lang: Language; compact?: boolean } = $props();
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users can inspect the timing method on focus.) -->
<span class="speed" class:compact tabindex={compact ? undefined : 0} data-tooltip={compact ? undefined : speedHint(value, lang)} aria-label={`TPS: ${formatTps(value, lang)}`}>
  {formatTps(value, lang, compact)}
</span>

<style>
  .speed { white-space: nowrap; color: var(--muted); font-size: 12px; font-weight: 500; font-variant-numeric: tabular-nums; }
  .speed.compact { color: inherit; font-size: inherit; font-weight: inherit; }
</style>
