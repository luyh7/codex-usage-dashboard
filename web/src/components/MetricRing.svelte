<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { ringArc } from '../lib/domain/ring';

  let { value, segmented = false, className }: {
    value: number | null;
    segmented?: boolean;
    className: string;
  } = $props();
  let shown = $state<number | null>(null);
  let mounted = $state(false);
  let reduced = $state(false);
  let target = $derived(value !== null && Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : null);

  onMount(() => {
    const preference = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => { reduced = preference.matches; };
    update();
    preference.addEventListener('change', update);
    mounted = true;
    return () => preference.removeEventListener('change', update);
  });
  $effect(() => {
    const next = target;
    if (!mounted) return;
    const from = untrack(() => shown);
    if (next === null || from === null || reduced || next === from) {
      shown = next;
      return;
    }
    // Match AnimatedValue's duration/easing, resuming from any interrupted frame.
    const start = performance.now();
    let frame = 0;
    const draw = (now: number) => {
      const progress = Math.min(1, (now - start) / 450);
      shown = from + (next - from) * (1 - (1 - progress) ** 3);
      if (progress < 1) frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  });
</script>

<svg class={`metric-ring ${className}`} viewBox="0 0 44 44" aria-hidden="true">
  <circle cx="22" cy="22" r="17" class={segmented ? 'session-ring-track' : 'ring-track'} />
  {#if segmented}
    <circle cx="22" cy="22" r="17" class="session-main-segment" opacity={shown === null ? 0 : 1} />
  {/if}
  <path
    class={segmented ? 'session-agent-segment' : 'ring-value'}
    d={ringArc(shown ?? 0, segmented ? 100 - (shown ?? 0) : 0)}
  />
</svg>
