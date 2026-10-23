<script lang="ts">
  import { words, type Language } from '../lib/domain/ui';
  import { sessionMetadata } from '../lib/domain/presentation';
  import ServiceTier from './ServiceTier.svelte';

  let { sessions, lang, longContext = false }: {
    sessions: Parameters<typeof sessionMetadata>[0];
    lang: Language;
    longContext?: boolean;
  } = $props();
  let t = $derived(words(lang));
  let metadata = $derived(sessionMetadata(sessions));
  let modelLabel = $derived(metadata.models.join(', ') || t.unknown);
  let effortLabel = $derived(metadata.efforts.length > 1 ? t.mixed : metadata.efforts[0] || t.unknown);
</script>

<span class="session-metadata">
  <span class="model-effort-tag">
    <span class="session-model" data-tooltip={modelLabel} data-tooltip-overflow>{modelLabel}</span>
    <span class="session-effort" data-tooltip={metadata.efforts.length > 1 ? metadata.efforts.join(', ') : undefined}>{effortLabel}</span>
  </span>
  {#each metadata.speedTiers as tier}<ServiceTier {tier} />{/each}
  {#if longContext}<span class="long-context-tag">{t.longContext}</span>{/if}
</span>
