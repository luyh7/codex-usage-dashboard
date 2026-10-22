<script lang="ts">
  import type { Usage } from '../lib/data/types';
  import { cacheHitPercent, cacheHitTone } from '../lib/domain/presentation';
  import { number, type Language } from '../lib/domain/ui';

  let { usage, lang }: { usage: Usage; lang: Language } = $props();
  let percent = $derived(cacheHitPercent(usage));
  let label = $derived(percent == null ? '—' : `${number(percent, lang)}%`);
</script>

<span class="cache-percent" data-cache-tone={cacheHitTone(percent)}>{label}</span>
