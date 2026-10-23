<script lang="ts">
  import type { Language } from '../lib/domain/ui';
  import Icon from './Icon.svelte';
  let { lang }: { lang: Language } = $props();
  let hint = $derived(lang === 'zh'
    ? '每秒输出的 token 数，越高越快。根据日志估算。'
    : 'Output tokens per second; higher is faster. Estimated from logs.');
</script>

<span class="speed-heading">tok/s<button type="button" class="speed-help"
  aria-label={lang === 'zh' ? '输出速度说明' : 'About output speed'} data-tooltip={hint}
><Icon name="alert-circle" size={12} /></button></span>

<style>
  .speed-heading { display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; }
  .speed-help { display: inline-flex; align-items: center; justify-content: center; padding: 2px; margin: -2px; border: 0; background: transparent; color: inherit; cursor: help; opacity: .8; }
  .speed-help:hover, .speed-help:focus-visible { opacity: 1; }
</style>
