<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import type { Daily } from '../lib/data/types';
  import { words, number, type Language } from '../lib/domain/ui';
  import { compactNumber } from '../lib/domain/presentation';
  import {
    todayInZone,
    parseDay,
    monthDays,
    rangeDays,
    presetRange,
    shiftDay,
    dayKey,
  } from '../lib/domain/calendar';
  import Icon from './Icon.svelte';
  import AnimatedValue from './AnimatedValue.svelte';
  import Dropdown from './Dropdown.svelte';
  import { dismissBackdrop } from '../lib/domain/dismiss';
  let {
    daily,
    loading,
    start,
    end,
    period,
    zone,
    lang,
    apply,
    close,
    trigger,
  }: {
    daily: Daily | null;
    loading: boolean;
    start: string;
    end: string;
    period: string;
    zone: string;
    lang: Language;
    apply: (start: string, end: string, period: string) => void;
    close: (restoreFocus?: boolean) => void;
    trigger: HTMLElement | undefined;
  } = $props();
  let t = $derived(words(lang));
  const initial = untrack(() => {
    let today: string;
    try {
      today = todayInZone(zone);
    } catch {
      today = todayInZone('UTC');
    }
    const from = parseDay(start) ? start : today;
    return { today, from, to: parseDay(end) ? end : from, period };
  });
  const today = initial.today;
  let from = $state(initial.from),
    to = $state(initial.to);
  let draftPeriod = $state(initial.period);
  let month = $state(initial.from.slice(0, 7));
  let focusDay = $state(initial.from);
  let choosingEnd = $state(false);
  let panel: HTMLDialogElement;
  let mobile = $state(false);
  let availableHeight = $state(600);
  let panelTop = $state(72),
    panelLeft = $state(12);
  let values = $derived(
    new Map((daily?.daily_usage || []).map((d) => [d.date, d.usage.total_tokens])),
  );
  let days = $derived(monthDays(Number(month.slice(0, 4)), Number(month.slice(5)) - 1));
  let validDays = $derived(rangeDays(from, to));
  let monthTotal = $derived(
    [...values].filter(([d]) => d.startsWith(month)).reduce((n, [, v]) => n + v, 0),
  );
  let yearTotal = $derived(
    [...values].filter(([d]) => d.startsWith(month.slice(0, 4))).reduce((n, [, v]) => n + v, 0),
  );
  let monthLabel = $derived(
    new Intl.DateTimeFormat(lang === 'zh' ? 'zh-CN' : 'en-US', {
      year: 'numeric',
      month: 'long',
      timeZone: 'UTC',
    }).format(parseDay(month + '-01')!),
  );
  let monthOptions = $derived(
    Array.from({ length: 12 }, (_, index): [string, string] => [
      String(index + 1).padStart(2, '0'),
      new Intl.DateTimeFormat(lang === 'zh' ? 'zh-CN' : 'en-US', { month: 'short', timeZone: 'UTC' })
        .format(new Date(Date.UTC(2000, index, 1))),
    ]),
  );
  let presets = $derived([
    ['today', t.today],
    ['7d', t.days7],
    ['week', t.week],
    ['month', t.month],
    ['30d', t.days30],
    ['all', t.all],
    ['custom', t.customShort],
  ]);

  onMount(() => {
    const media = matchMedia('(max-width: 760px)');
    const update = () => {
      mobile = media.matches;
      const anchor = trigger?.getBoundingClientRect();
      const width = Math.min(712, innerWidth - 32);
      panelTop = Math.min((anchor?.bottom ?? 64) + 8, Math.max(12, innerHeight - 560));
      panelLeft = Math.max(
        12,
        Math.min((anchor?.right ?? innerWidth - 12) - width, innerWidth - width - 12),
      );
      availableHeight = Math.max(80, innerHeight - panelTop - 12);
    };
    update();
    panel.showModal();
    window.addEventListener('resize', update);
    void tick().then(() =>
      panel.querySelector<HTMLElement>(`[data-date="${focusDay}"]`)?.focus({ preventScroll: true }),
    );
    return () => {
      window.removeEventListener('resize', update);
      panel.close();
    };
  });
  $effect(() => {
    if (!mobile) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = previous;
    };
  });
  function focusDate() {
    const button = panel.querySelector<HTMLElement>(`[data-date="${focusDay}"]`);
    button?.focus({ preventScroll: true });
    const body = panel.querySelector<HTMLElement>('.calendar-body');
    if (!button || !body || body.scrollHeight <= body.clientHeight) return;
    const target = button.getBoundingClientRect(),
      viewport = body.getBoundingClientRect();
    if (target.bottom > viewport.bottom) body.scrollTop += target.bottom - viewport.bottom + 8;
    else if (target.top < viewport.top) body.scrollTop -= viewport.top - target.top + 8;
  }
  function keyboard(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopImmediatePropagation();
      close();
    }
    if (event.key !== 'Tab') return;
    const items = Array.from(
      panel.querySelectorAll<HTMLElement>(
        'button:not(:disabled), input:not(:disabled), summary, [tabindex="0"]',
      ),
    ).filter((el) => el.getClientRects().length);
    const first = items[0],
      last = items.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
  function choose(date: string) {
    draftPeriod = 'custom';
    focusDay = date;
    if (!choosingEnd) {
      from = date;
      to = date;
      choosingEnd = true;
    } else {
      [from, to] = date < from ? [date, from] : [from, date];
      choosingEnd = false;
    }
  }
  function preset(value: string) {
    draftPeriod = value;
    choosingEnd = false;
    if (value === 'custom') return;
    const earliest = [...values.keys()].sort()[0] || today;
    ({ from, to } = presetRange(value, today, earliest));
    month = from.slice(0, 7);
    focusDay = from;
  }
  function move(delta: number) {
    const date = parseDay(month + '-01')!;
    date.setUTCMonth(date.getUTCMonth() + delta);
    month = dayKey(date).slice(0, 7);
    focusDay = month + '-01';
  }
  async function dayKeyboard(event: KeyboardEvent, date: string) {
    let next = date;
    if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key))
      next = shiftDay(
        date,
        { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[event.key]!,
      );
    else if (event.key === 'Home') next = shiftDay(date, -((parseDay(date)!.getUTCDay() + 6) % 7));
    else if (event.key === 'End')
      next = shiftDay(date, 6 - ((parseDay(date)!.getUTCDay() + 6) % 7));
    else if (event.key === 'PageUp' || event.key === 'PageDown') {
      move(event.key === 'PageUp' ? -1 : 1);
      next = month + '-01';
    } else return;
    event.preventDefault();
    month = next.slice(0, 7);
    focusDay = next;
    await tick();
    focusDate();
  }
  function edited() {
    draftPeriod = 'custom';
    choosingEnd = false;
    if (parseDay(from)) {
      month = from.slice(0, 7);
      focusDay = from;
    }
  }
</script>

<svelte:window onkeydown={keyboard} />
<dialog
  bind:this={panel!}
  use:dismissBackdrop={() => close()}
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  id="date-picker"
  class="calendar-panel"
  style:--calendar-available-height={availableHeight + 'px'}
  style:--calendar-top={panelTop + 'px'}
  style:--calendar-left={panelLeft + 'px'}
  aria-modal="true"
  aria-labelledby="calendar-title"
  tabindex="-1"
>
  <header class="calendar-heading">
    <h2 id="calendar-title">{t.selectDates}</h2>
    <button class="calendar-mobile-close icon-button" aria-label={t.close} onclick={() => close()}
      ><Icon name="close" /></button
    >
  </header>
  <div class="calendar-body">
    <nav class="calendar-presets" aria-label={t.period}>
      {#each presets as [value, label]}<button
          class:chosen={draftPeriod === value}
          aria-pressed={draftPeriod === value}
          onclick={() => preset(value)}>{label}</button
        >{/each}
    </nav>
    <div class="calendar-month">
      <div class="calendar-nav">
        <button class="month-previous" aria-label={t.previousMonth} onclick={() => move(-1)}
          ><Icon name="chevron" size={16} /></button
        >
        <div class="calendar-month-selectors">
          <input class="calendar-year-input" aria-label={t.year} inputmode="numeric" maxlength="4" value={month.slice(0, 4)} onchange={(event) => {
            const year = event.currentTarget.value;
            if (/^\d{4}$/.test(year) && Number(year) >= 1970 && parseDay(`${year}-${month.slice(5)}-01`)) {
              month = `${year}-${month.slice(5)}`;
              focusDay = month + '-01';
            } else event.currentTarget.value = month.slice(0, 4);
          }} />
          <div class="calendar-month-dropdown"><Dropdown label={t.monthPicker} value={month.slice(5)} options={monthOptions} change={(value) => { month = `${month.slice(0, 4)}-${value}`; focusDay = month + '-01'; }} /></div>
        </div>
        <button aria-label={t.nextMonth} onclick={() => move(1)}
          ><Icon name="chevron" size={16} /></button
        >
        <button
          class="back-today"
          onclick={() => {
            month = today.slice(0, 7);
            focusDay = today;
          }}>{t.backToday}</button
        >
      </div>
      <div class="calendar-weekdays" aria-hidden="true">
        {#each lang === 'zh' ? ['一', '二', '三', '四', '五', '六', '日'] : ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'] as day}<span
            >{day}</span
          >{/each}
      </div>
      <div class="calendar-grid" aria-label={monthLabel}>
        {#each days as date, index (date)}{@const inRange =
            !!validDays && date >= from && date <= to}
          <button
            data-date={date}
            style:--range-column={index % 7}
            class:outside={!date.startsWith(month)}
            class:in-range={inRange}
            class:range-start={date === from}
            class:range-end={date === to}
            class:today={date === today}
            tabindex={focusDay === date ? 0 : -1}
            aria-current={date === today ? 'date' : undefined}
            aria-pressed={inRange}
            aria-label={`${date}: ${loading ? t.loading : number(values.get(date), lang) + ' tokens'}`}
            onfocus={() => (focusDay = date)}
            onkeydown={(e) => dayKeyboard(e, date)}
            onclick={() => choose(date)}
          >
            <span class="date-number">{Number(date.slice(8))}</span><small
              >{#if loading}…{:else}<AnimatedValue
                  value={values.get(date) ?? 0}
                  format={(value) => (value ? compactNumber(value) : '')}
                />{/if}</small
            >
          </button>
        {/each}
      </div>
      <div class="calendar-totals">
        <span>{t.dailyUsage} · tokens</span><span
          >{t.month}
          <b data-tooltip={number(monthTotal, lang)}
            >{#if loading}—{:else}<AnimatedValue
                value={monthTotal}
                format={compactNumber}
              />{/if}</b
          ><i></i>{t.thisYear}
          <b data-tooltip={number(yearTotal, lang)}
            >{#if loading}—{:else}<AnimatedValue value={yearTotal} format={compactNumber} />{/if}</b
          ></span
        >
      </div>
    </div>
  </div>
  <form
    class="date-inputs"
    onsubmit={(e) => {
      e.preventDefault();
      if (validDays) apply(from, to, draftPeriod);
    }}
  >
    <label
      >{t.from}<span class="date-field"
        ><Icon name="calendar" size={18} /><input
          aria-label={t.from}
          type="text"
          inputmode="numeric"
          placeholder="YYYY/MM/DD"
          autocomplete="off"
          required
          value={from.replaceAll('-', '/')}
          oninput={(e) => {
            from = e.currentTarget.value.replaceAll('/', '-');
            edited();
          }}
          aria-invalid={!!from && !!to && !validDays}
        /></span
      ></label
    >
    <label
      >{t.to}<span class="date-field"
        ><Icon name="calendar" size={18} /><input
          aria-label={t.to}
          type="text"
          inputmode="numeric"
          placeholder="YYYY/MM/DD"
          autocomplete="off"
          required
          value={to.replaceAll('-', '/')}
          oninput={(e) => {
            to = e.currentTarget.value.replaceAll('/', '-');
            edited();
          }}
          aria-invalid={!!from && !!to && !validDays}
        /></span
      ></label
    >
    <span class="selected-days" aria-live="polite"
      >{validDays
        ? lang === 'zh'
          ? `已选 ${validDays} 天`
          : `${validDays} days selected`
        : t.invalidRange}</span
    >
    <button type="button" onclick={() => close()}>{t.cancel}</button><button
      class="primary"
      type="submit"
      disabled={!validDays}>{t.apply}</button
    >
  </form>
</dialog>
