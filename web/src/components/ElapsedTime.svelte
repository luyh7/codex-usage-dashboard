<script lang="ts">
  import { onMount } from 'svelte';
  import { words, type Language } from '../lib/domain/ui';

  let { lang, startedAt = Date.now() }: { lang: Language; startedAt?: number } = $props();
  let seconds = $state(0);
  let display = $derived(`${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`);
  onMount(() => {
    const update = () => { seconds = Math.max(0, Math.floor((Date.now() - startedAt) / 1000)); };
    update();
    const timer = setInterval(update, 1000);
    return () => clearInterval(timer);
  });
</script>

<span class="elapsed" aria-hidden="true">{words(lang).waited} {display}</span>

<style>
  .elapsed { color: #718e82; font-size: 11px; font-variant-numeric: tabular-nums; white-space: nowrap; }
</style>
