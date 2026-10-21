<script lang="ts">
  import { onMount } from 'svelte';
  let { value, format }: { value: number | null; format: (value: number) => string } = $props();
  let shown = $state<number | null>(null);
  let mounted = $state(false);
  let reduced = $state(false);
  let previous: number | null = null;
  onMount(() => {
    const preference = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => {
      reduced = preference.matches;
    };
    update();
    preference.addEventListener('change', update);
    mounted = true;
    return () => preference.removeEventListener('change', update);
  });
  $effect(() => {
    const target = value;
    if (!mounted) return;
    if (target === null || previous === null || reduced || target === previous) {
      shown = previous = target;
      return;
    }
    const from = previous;
    previous = target;
    const start = performance.now();
    let frame = 0;
    const draw = (now: number) => {
      const progress = Math.min(1, (now - start) / 450);
      shown = from + (target - from) * (1 - (1 - progress) ** 3);
      if (progress < 1) frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  });
</script>

<span class="animated-value" aria-label={value === null ? '—' : format(value)}>
  <span aria-hidden="true">{shown === null ? '—' : format(shown)}</span>
</span>
