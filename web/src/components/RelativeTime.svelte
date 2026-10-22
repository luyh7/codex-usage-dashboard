<script module lang="ts">
  import { readable } from 'svelte/store';
  // All visible timestamps share one clock and one timer.
  const clock = readable(Date.now(), (set) => {
    set(Date.now());
    const timer = setInterval(() => set(Date.now()), 30000);
    return () => clearInterval(timer);
  });
</script>

<script lang="ts">
  import { relativeTime } from '../lib/domain/presentation';
  import { dateTime, type Language } from '../lib/domain/ui';
  let { value, lang, zone }: { value: string | null | undefined; lang: Language; zone: string } = $props();
</script>

<time class="relative-time" datetime={value || undefined} data-tooltip={value ? dateTime(value, zone, lang) : undefined}
  >{relativeTime(value, lang, $clock)}</time
>
