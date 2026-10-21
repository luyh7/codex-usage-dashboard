<script lang="ts">
  import type { Daily } from '../lib/data/types';
  import { words, number, type Language } from '../lib/domain/ui';
  let {
    daily,
    loading,
    start,
    end,
    lang,
    apply,
  }: {
    daily: Daily | null;
    loading: boolean;
    start: string;
    end: string;
    lang: Language;
    apply: (start: string, end: string) => void;
  } = $props();
  let t = $derived(words(lang));
  const initial = new Date();
  let year = $state(initial.getFullYear()),
    month = $state(initial.getMonth());
  let from = $state(''),
    to = $state('');
  $effect(() => {
    from = start;
    to = end;
    if (start) {
      const date = new Date(start + 'T12:00:00');
      if (!Number.isNaN(+date)) {
        year = date.getFullYear();
        month = date.getMonth();
      }
    }
  });
  let choosingEnd = $state(false);
  let values = $derived(
    new Map((daily?.daily_usage || []).map((d) => [d.date, d.usage.total_tokens])),
  );
  const key = (date: Date) =>
    `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
  let days = $derived.by(() => {
    const first = new Date(year, month, 1);
    const start = new Date(year, month, 1 - ((first.getDay() + 6) % 7));
    return Array.from(
      { length: 42 },
      (_, i) => new Date(start.getFullYear(), start.getMonth(), start.getDate() + i),
    );
  });
  let monthTotal = $derived(
    [...values.entries()]
      .filter(([d]) => d.startsWith(`${year}-${String(month + 1).padStart(2, '0')}`))
      .reduce((n, [, v]) => n + v, 0),
  );
  let yearTotal = $derived(
    [...values.entries()].filter(([d]) => d.startsWith(year + '-')).reduce((n, [, v]) => n + v, 0),
  );
  function choose(date: string) {
    if (!choosingEnd) {
      from = date;
      to = date;
      choosingEnd = true;
    } else {
      if (date < from) {
        to = from;
        from = date;
      } else to = date;
      choosingEnd = false;
    }
  }
  function move(delta: number) {
    const date = new Date(year, month + delta, 1);
    year = date.getFullYear();
    month = date.getMonth();
  }
</script>

<section class="calendar-panel">
  <div class="calendar-nav">
    <button aria-label={t.previous} onclick={() => move(-1)}>‹</button><label
      >{t.year}<input
        aria-label={t.year}
        type="number"
        min="1970"
        max="9999"
        bind:value={year}
      /></label
    ><label
      >{t.monthPicker}<select aria-label={t.monthPicker} bind:value={month}
        >{#each Array.from({ length: 12 }, (_, i) => i) as m}<option value={m}>{m + 1}</option
          >{/each}</select
      ></label
    ><button aria-label={t.next} onclick={() => move(1)}>›</button>
  </div>
  <div class="calendar-totals">
    {t.month}: {loading ? t.loading : number(monthTotal, lang)} · {t.year}: {loading
      ? t.loading
      : number(yearTotal, lang)} tokens
  </div>
  <div class="calendar-grid">
    {#each lang === 'zh' ? ['一', '二', '三', '四', '五', '六', '日'] : ['M', 'T', 'W', 'T', 'F', 'S', 'S'] as day}<span
        class="weekday">{day}</span
      >{/each}{#each days as date (key(date))}{@const dateKey = key(date)}<button
        class:outside={date.getMonth() !== month}
        class:in-range={dateKey >= from && dateKey <= to}
        class:has-usage={!!values.get(dateKey)}
        aria-label={`${dateKey}: ${loading ? t.loading : number(values.get(dateKey), lang) + ' tokens'}`}
        aria-pressed={dateKey >= from && dateKey <= to}
        onclick={() => choose(dateKey)}
        ><span>{date.getDate()}</span><small
          >{loading ? '…' : values.get(dateKey) ? number(values.get(dateKey), lang) : '·'}</small
        ></button
      >{/each}
  </div>
  <p class="muted">{t.dateHint}</p>
  <div class="date-inputs">
    <label>{t.from}<input type="date" bind:value={from} /></label><label
      >{t.to}<input type="date" bind:value={to} /></label
    ><button class="primary" disabled={!from || !to} onclick={() => apply(from, to)}
      >{t.apply}</button
    >
  </div>
</section>
