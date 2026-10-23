<script lang="ts">
  import { untrack, onMount, tick } from 'svelte';
  import type { DashboardData, DetailBundle, Events } from '../lib/data/types';
  import { cancelled, DataError } from '../lib/data/http';
  import { words, number, money, dateTime, models, type Language } from '../lib/domain/ui';
  import UsageChart from './UsageChart.svelte';
  import Icon from './Icon.svelte';
  import AnimatedValue from './AnimatedValue.svelte';
  import { compactNumber, displayMoney, costSegments, serviceTier } from '../lib/domain/presentation';
  import ArchiveTag from './ArchiveTag.svelte';
  import SessionMetadata from './SessionMetadata.svelte';
  import ServiceTier from './ServiceTier.svelte';
  import RelativeTime from './RelativeTime.svelte';
  import UsageBreakdown from './UsageBreakdown.svelte';
  import { metricTooltip } from '../lib/domain/metricTooltip';
  import { segmentPriceTooltip } from '../lib/domain/usageBreakdown';
  let {
    bundle,
    showEnvironment,
    projectView,
    loading,
    active = true,
    error,
    retryable = true,
    lang,
    zone,
    data,
    close,
    retry,
    onerror,
  }: {
    bundle: DetailBundle | null;
    showEnvironment: boolean;
    projectView: boolean;
    loading: boolean;
    active?: boolean;
    error: string;
    retryable?: boolean;
    lang: Language;
    zone: string;
    data: DashboardData;
    close: () => void;
    retry: () => void;
    onerror: (error: unknown) => void;
  } = $props();
  let t = $derived(words(lang));
  let page = $state.raw<Events | null>(null);
  let paging = $state(false);
  let eventError = $state('');
  let eventGeneration = 0;
  let expanded = $state<'events' | 'more' | null>(null);
  let disclosureSequence = 0;
  let scrollRoot = $state<HTMLDivElement>();
  let copied = $state(false);
  let d = $derived(bundle?.detail);
  let panel: HTMLElement;
  let mobile = $state(false);
  let segments = $derived(d ? costSegments(d) : []);
  let segmentTotal = $derived(segments.reduce((sum, part) => sum + part.value, 0));
  const eventFields = ['total_tokens', 'input_tokens', 'cached_input_tokens', 'output_tokens', 'reasoning_output_tokens'] as const;
  let eventLabels = $derived(['Tokens', t.input, t.cacheCounts, t.output, t.reasoningShort]);
  onMount(() => {
    const media = matchMedia('(max-width: 1100px)');
    const update = () => {
      const entering = media.matches && !mobile;
      mobile = media.matches;
      if (active && entering) void tick().then(() => { if (active) panel?.focus(); });
    };
    update();
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  });
  function keyboard(event: KeyboardEvent) {
    if (!active) return;
    if (document.querySelector('dialog[open], .calendar-panel')) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
    }
    if (event.key !== 'Tab' || !mobile) return;
    const focusable = Array.from(
      panel.querySelectorAll<HTMLElement>(
        'button:not(:disabled), a[href], input, select, summary, [tabindex="0"]',
      ),
    ).filter((el) => el.getClientRects().length);
    const first = focusable[0],
      last = focusable.at(-1);
    if (!first || !last) return;
    if (event.shiftKey && (document.activeElement === first || document.activeElement === panel)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
  $effect(() => {
    if (!active || !mobile) return;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = overflow;
    };
  });
  $effect(() => {
    const next = bundle;
    untrack(() => {
      const generation = ++eventGeneration;
      const sameSession = !!next && page?.id === next.detail.uid;
      const count = sameSession ? page?.events.length || 0 : 0;
      eventError = '';
      paging = false;
      copied = false;
      if (!sameSession) {
        disclosureSequence++;
        expanded = null;
        scrollRoot?.scrollTo({ top: 0, behavior: 'instant' });
      }
      if (next && count > next.events.events.length) {
        // Keep the visible list while rebuilding it against the new snapshot.
        void restoreEvents(next.events, count, generation);
      } else {
        page = next?.events || null;
      }
    });
    return () => { eventGeneration++; };
  });

  function appendEvents(current: Events, next: Events): Events {
    if (next.offset !== current.events.length || !next.events.length) {
      throw new Error('Event stream did not advance');
    }
    return { ...next, offset: 0, previous_cursor: null, events: [...current.events, ...next.events] };
  }

  async function restoreEvents(initial: Events, count: number, generation: number) {
    paging = true;
    try {
      let restored = initial;
      while (restored.events.length < count && restored.next_cursor) {
        const result = await data.events(initial.id, initial.revision, restored.next_cursor);
        if (generation !== eventGeneration) return;
        restored = appendEvents(restored, result);
      }
      if (generation === eventGeneration) page = restored;
    } catch (e) {
      if (generation === eventGeneration) {
        page = initial;
        if (!cancelled(e)) {
          handleEventError(e);
        }
      }
    } finally {
      if (generation === eventGeneration) paging = false;
    }
  }

  async function loadMoreEvents() {
    if (paging || !page?.next_cursor || page.revision !== d?.revision) return;
    const current = page;
    const generation = eventGeneration;
    paging = true;
    eventError = '';
    try {
      const result = await data.events(current.id, current.revision, current.next_cursor!);
      if (generation === eventGeneration) page = appendEvents(current, result);
    } catch (e) {
      if (generation === eventGeneration && !cancelled(e)) {
        handleEventError(e);
      }
    } finally {
      if (generation === eventGeneration) paging = false;
    }
  }

  function handleEventError(error: unknown) {
    eventError = error instanceof Error ? error.message : String(error);
    if (error instanceof DataError && error.stale) onerror(error);
  }

  function duration(milliseconds: number) {
    const seconds = Math.max(0, milliseconds / 1000);
    const unit = seconds >= 3600 ? 'hour' : seconds >= 60 ? 'minute' : 'second';
    const value = seconds / (unit === 'hour' ? 3600 : unit === 'minute' ? 60 : 1);
    return new Intl.NumberFormat(lang === 'zh' ? 'zh-CN' : 'en-US', {
      style: 'unit', unit, unitDisplay: 'short', maximumFractionDigits: 1,
    }).format(value);
  }

  function sizeScrollArea(node: HTMLDivElement) {
    const observer = new ResizeObserver(() => {
      node.style.setProperty('--detail-viewport-height', `${node.clientHeight}px`);
    });
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }

  function watchEventsEnd(node: HTMLDivElement) {
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && !eventError) void loadMoreEvents();
    }, { root: scrollRoot, rootMargin: '0px 0px 200px 0px' });
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }

  function animateDisclosure(node: HTMLDetailsElement, initial: { open: boolean; id: string }) {
    let state = initial;
    let motion: Animation | null = null;
    let fade: Animation | null = null;
    node.open = state.open;
    const cancel = () => {
      motion?.cancel();
      fade?.cancel();
      motion = fade = null;
      node.classList.remove('disclosure-animating');
    };
    return {
      update(next: typeof initial) {
        if (next.id === state.id && next.open === state.open) return;
        const previous = state;
        state = next;
        const content = node.querySelector<HTMLElement>(':scope > div');
        const start = node.getBoundingClientRect().height;
        const opacity = node.open && content ? getComputedStyle(content).opacity : '0';
        cancel();
        if (next.id !== previous.id || matchMedia('(prefers-reduced-motion: reduce)').matches) {
          node.open = next.open;
          return;
        }
        // Keep native details open until its closing transition completes.
        node.open = true;
        const style = getComputedStyle(node);
        const closed = node.querySelector('summary')!.getBoundingClientRect().height
          + parseFloat(style.paddingTop) + parseFloat(style.paddingBottom)
          + parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth);
        const end = next.open ? node.getBoundingClientRect().height : closed;
        node.classList.add('disclosure-animating');
        const transition = node.animate([
          { height: `${start}px`, minHeight: '0px' },
          { height: `${end}px`, minHeight: '0px' },
        ], { duration: 280, easing: 'cubic-bezier(.2, .8, .2, 1)', fill: 'both' });
        motion = transition;
        fade = content?.animate([
          { opacity }, { opacity: next.open ? '1' : '0' },
        ], { duration: next.open ? 220 : 180, easing: 'ease-out', fill: 'both' }) || null;
        void transition.finished.then(() => {
          if (motion !== transition) return;
          node.open = state.open;
          cancel();
        }, () => { /* A newer click or session superseded this transition. */ });
      },
      destroy: cancel,
    };
  }

  async function toggleSection(event: MouseEvent, section: 'events' | 'more') {
    event.preventDefault();
    const heading = event.currentTarget as HTMLElement;
    const sequence = ++disclosureSequence;
    expanded = expanded === section ? null : section;
    if (!expanded) return;
    await tick();
    if (!scrollRoot) return;
    const animations = Array.from(scrollRoot.querySelectorAll(':scope > details'))
      .flatMap((element) => element.getAnimations());
    const disclosure = heading.parentElement!;
    const style = getComputedStyle(disclosure);
    const current = () => scrollRoot && heading.isConnected && expanded === section && sequence === disclosureSequence;
    const target = () => scrollRoot!.scrollTop + disclosure.getBoundingClientRect().top - scrollRoot!.getBoundingClientRect().top
      + parseFloat(style.paddingTop) + parseFloat(style.borderTopWidth);
    const start = scrollRoot.scrollTop;
    const startedAt = performance.now();
    const duration = matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 220;
    const advance = (now: number) => {
      if (!current()) return;
      const progress = duration ? Math.min(1, (now - startedAt) / duration) : 1;
      scrollRoot!.scrollTo({ top: start + (target() - start) * (1 - (1 - progress) ** 3), behavior: 'instant' });
      if (progress < 1) requestAnimationFrame(advance);
    };
    if (duration) requestAnimationFrame(advance);
    else advance(startedAt);
    // Opening and scrolling run together; settle against the final layout.
    await Promise.allSettled(animations.map((animation) => animation.finished));
    if (current()) scrollRoot!.scrollTo({ top: target(), behavior: 'instant' });
  }
</script>

<svelte:window onkeydown={keyboard} />
<button class="detail-backdrop" aria-label={t.closeDetails} tabindex="-1" onclick={close}></button>
<aside
  bind:this={panel!}
  class="detail-panel"
  tabindex="-1"
  role={mobile ? 'dialog' : 'complementary'}
  aria-modal={mobile ? true : undefined}
  aria-label={t.details}
  aria-busy={loading}
>
  <header class="detail-heading">
    <div>
      <h2 data-tooltip={d?.title} data-tooltip-overflow>{d?.title || (loading ? t.loading : t.details)}</h2>
      {#if d && projectView}<span class="role-tag" class:main-role-tag={!d.is_subagent}>{d.is_subagent ? t.agentLabel : t.mainThread}</span
        >{/if}
    </div>
    <button class="icon-button" aria-label={t.close} onclick={close}
      ><Icon name="close" size={19} /></button
    >
  </header>
  {#if loading && !bundle}<div class="detail-skeleton" role="status" aria-label={t.loading}>
      <span></span><span></span><span></span>
    </div>{/if}
  {#if error}<p class="error" role="alert">
      {error} {#if retryable}<button onclick={retry}>{t.retry}</button>{/if}
    </p>{/if}
  {#if d && bundle}<div class="detail-scroll" bind:this={scrollRoot} use:sizeScrollArea>
      <div class="detail-tags">
        <ArchiveTag source={d.source} {lang} />
        <span class="detail-project-tag"><Icon name="folder" size={14} /><span data-tooltip={d.project} data-tooltip-overflow>{d.project}</span></span>
        <SessionMetadata sessions={[d]} {lang} />
        {#if showEnvironment}<span class="environment-badge" data-environment={d.environment}>{d.environment}</span>{/if}
      </div>
      <div class="detail-metrics">
        <div class="detail-cost">
          <small>{t.cost}</small><strong
            data-tooltip={money(d.estimated_cost_usd, lang)}
            style:--value-length={d.estimated_cost_usd == null
              ? 8
              : displayMoney(d.estimated_cost_usd).length}
            >{#if d.estimated_cost_usd == null}{t.unknown}{:else}<AnimatedValue
                value={d.estimated_cost_usd}
                format={displayMoney}
              />{/if}</strong
          >
        </div>
        <div class="detail-usage" data-tooltip={number(d.total_token_usage?.total_tokens, lang) !== compactNumber(d.total_token_usage?.total_tokens) ? number(d.total_token_usage?.total_tokens, lang) : undefined}>
          <AnimatedValue value={d.total_token_usage?.total_tokens ?? 0} format={compactNumber} /><span>tokens</span>
        </div>
      </div>
      {#if projectView && (d.is_subagent || d.has_subagents)}<p class="own-usage-hint">{d.is_subagent ? t.agentHint : t.ownHint}</p>{/if}
      {#if d.inherited_token_event_count > 0}<p class="notice">
          {t.inherited}: <AnimatedValue
            value={d.inherited_token_usage?.total_tokens ?? 0}
            format={(value) => number(Math.round(value), lang)}
          /> tokens
        </p>{/if}
      {#if d.is_subagent && !d.fork_usage_resolved}<p class="notice">{t.unresolved}</p>{/if}
      <UsageBreakdown detail={d} {lang} {zone} />
      <UsageChart data={bundle.chart} {lang} {zone} />
      <section class="cost-composition">
        <h3>{t.costComposition}</h3>
        {#if segments.length}
          <div class="cost-bar" aria-hidden="true">
            {#each segments as part}<span
                style:background={part.color}
                style:width={segmentTotal ? (part.value / segmentTotal) * 100 + '%' : '0%'}
              ></span>{/each}
          </div>
          <div class="cost-legend" style:--cost-columns={segments.length}>
            {#each segments as part (part.key)}<div
                style:--cost-value-length={displayMoney(part.value).length}
              >
                <span class="cost-label"><i style:background={part.color}></i>{t[part.key]}</span
                ><strong data-tooltip={money(part.value, lang)}
                  ><AnimatedValue value={part.value} format={displayMoney} /></strong
                ><small
                  ><AnimatedValue
                    value={segmentTotal ? (part.value / segmentTotal) * 100 : 0}
                    format={(value) => Math.round(value) + '%'}
                  /></small
                >
              </div>{/each}
          </div>
        {:else}<p class="muted">{t.unknownHint}</p>{/if}
      </section>
      <details class="events-section" use:animateDisclosure={{ open: expanded === 'events', id: d.uid }}>
        <summary onclick={(event) => toggleSection(event, 'events')}
          ><Icon name="events" size={23} /><span>{t.events}</span><span class="detail-count"
            ><AnimatedValue
              value={page?.total ?? 0}
              format={(value) => number(Math.round(value), lang)}
            /></span
          ><Icon name="chevron" size={17} /></summary
        >
        <div class="event-content" aria-busy={paging}>
          {#if page?.events.length}<div class="table-scroll event-scroll">
              <table>
                <thead
                  ><tr
                    ><th>{t.updated}</th><th>{t.model}</th><th class="numeric">Tokens</th><th class="numeric">{t.input}</th><th class="numeric">{t.cacheCounts}</th><th class="numeric">{t.output}</th><th class="numeric">{t.reasoningShort}</th></tr
                  ></thead
                ><tbody
                  >{#each page.events as row}<tr
                      ><td class="event-time"><RelativeTime value={row.timestamp} {lang} {zone} /></td><td class="event-model-cell"><span class="event-model">
                        <span class="model-effort-tag"><span class="session-model" data-tooltip={row.model || d.model} data-tooltip-overflow>{row.model || d.model}</span><span class="session-effort">{d.effort || t.unknown}</span></span>
                        {#if ['fast', 'ultrafast'].includes(serviceTier(row.service_tier))}<ServiceTier tier={row.service_tier} />{/if}
                      </span></td>
                      {#each eventFields as field, i}<td class="numeric" data-tooltip={number(row.last_token_usage?.[field], lang)}><span class="event-field-label">{eventLabels[i]}</span><span class="event-field-value">{compactNumber(row.last_token_usage?.[field])}</span></td>{/each}
                      </tr
                    >{/each}</tbody
                >
              </table>
            </div>{:else}<p class="muted">{t.noEvents}</p>{/if}
          {#if eventError}<div class="event-load-state" role="alert"><span>{eventError}</span><button onclick={loadMoreEvents}>{t.retry}</button></div>
          {:else if paging}<div class="event-load-state" role="status">{t.loading}</div>
          {:else if expanded === 'events' && page?.next_cursor}<div class="event-sentinel" use:watchEventsEnd></div>{/if}
        </div>
      </details>

      <details class="more-information" class:events-footer={expanded === 'events'} use:animateDisclosure={{ open: expanded === 'more', id: d.uid }}>
        <summary onclick={(event) => toggleSection(event, 'more')}
          ><Icon name="info" size={23} /><span>{t.moreInfo}</span><Icon
            name="chevron"
            size={17}
          /></summary
        >
        <div class="more-information-content">
          {#if d.first_user_prompt}<section class="information-section message-section">
              <h4>{t.prompt}</h4>
              <pre>{d.first_user_prompt}</pre>
            </section>{/if}{#if d.last_agent_preview}<section class="information-section message-section">
              <h4>{t.response}</h4>
              <pre>{d.last_agent_preview}</pre>
            </section>{/if}
          <section class="information-section">
            <h4>{t.prices}</h4>
            {#each d.applied_price_segments || [] as segment}<div class="price-row">
                <SessionMetadata sessions={[{
                  ...d,
                  model: segment.model || d.model,
                  models: [segment.model || d.model],
                  effort: typeof segment.effort === 'string' ? segment.effort : d.effort,
                  service_tier: segment.service_tier,
                }]} {lang} longContext={segment.context_tier === 'long'} />
                <span class="price-tokens" data-tooltip={`${number(segment.usage?.total_tokens, lang)} tokens`}>{compactNumber(segment.usage?.total_tokens)}</span>
                <button type="button" class="segment-price" aria-label={t.prices}
                  use:metricTooltip={segmentPriceTooltip(d, segment, lang, zone)}
                  >{segment.estimated_cost_usd == null ? t.unknown : displayMoney(segment.estimated_cost_usd)}</button>
              </div>{:else}<p class="muted">{t.unknownHint}</p>{/each}
          </section>
          {#if Object.keys(d.tool_counts || {}).length}<section class="information-section">
            <h4>{t.tools}</h4>
            <dl class="tool-counts">{#each Object.entries(d.tool_counts || {}) as [name, count]}<div>
                <dt>{name}</dt><dd>{number(count, lang)}</dd>
              </div>{/each}</dl>
          </section>{/if}
          <section class="information-section">
            <h4>{t.metadata}</h4>
            <dl class="session-information">
              <dt>Session ID</dt>
              <dd class="technical-value">{d.session_id}</dd>
              <dt>{t.path}</dt>
              <dd class="copyable-value">
                <span class="technical-value">{d.path}</span>
                <button
                  class="copy-path"
                  aria-label={copied ? t.copied : t.copy}
                  data-tooltip={copied ? t.copied : t.copy}
                  onclick={async () => {
                    await navigator.clipboard.writeText(d.path);
                    copied = true;
                  }}><Icon name={copied ? 'check' : 'copy'} size={14} /></button
                >
              </dd>
              <dt>{t.projects}</dt>
              <dd class="technical-value">{d.cwd}</dd>
              <dt>{t.updated}</dt>
              <dd>{dateTime(d.end_at, zone, lang)}</dd>
              <dt>{t.duration}</dt>
              <dd>{duration(d.duration_ms_total)}</dd>
              <dt>{t.turns}</dt>
              <dd>{number(d.completed_turn_count, lang)} / {number(d.turn_count, lang)}</dd>
              <dt>{t.environment}</dt>
              <dd>{d.environment}</dd>
              <dt>{t.model}</dt>
              <dd class="information-model"><SessionMetadata sessions={[d]} {lang} /></dd>
              <dt>CLI</dt>
              <dd>{d.cli_version || '—'}</dd>
              <dt>{t.parseErrors}</dt>
              <dd>{d.parse_errors || 0}</dd>
            </dl>
          </section>
        </div>
      </details>
    </div>{/if}
</aside>
