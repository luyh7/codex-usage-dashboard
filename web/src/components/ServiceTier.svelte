<script lang="ts">
  import { serviceTier, serviceTierLabel } from '../lib/domain/presentation';
  import Icon from './Icon.svelte';

  let { tier }: { tier?: string | null } = $props();
  let mode = $derived(serviceTier(tier));
  let accelerated = $derived(mode === 'fast' || mode === 'ultrafast');
</script>

<span class="service-tier" class:accelerated data-service-tier={mode}>
  {#if accelerated}<Icon name="lightning" size={12} />{/if}
  {serviceTierLabel(tier)}
</span>

<style>
  .service-tier {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
    white-space: nowrap;
  }
  .accelerated {
    box-sizing: border-box;
    height: var(--metadata-tag-height, 24px);
    min-height: var(--metadata-tag-height, 24px);
    padding: 2px 6px;
    border: 1px solid #eadbb9;
    border-radius: 6px;
    background: #fff7e6;
    color: #95600b;
    line-height: 1.4;
  }
</style>
