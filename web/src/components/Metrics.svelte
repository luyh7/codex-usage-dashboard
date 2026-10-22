<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import type { Summary, MetricTrend, Period } from '../lib/data/types';
  import { words, number, money, type Language } from '../lib/domain/ui';
  import { compactNumber, displayMoney, cacheHitPercent, cacheHitTone } from '../lib/domain/presentation';
  import AnimatedValue from './AnimatedValue.svelte';
  import MetricRing from './MetricRing.svelte';
  import Icon from './Icon.svelte';
  import MetricSpark from './MetricSpark.svelte';
  import DailyUsage from './DailyUsage.svelte';
  import ModelBreakdown from './ModelBreakdown.svelte';
  import { metricSeries } from '../lib/domain/metrics';
  import { dismissDetails } from '../lib/domain/dismiss';
  import { metricTooltip } from '../lib/domain/metricTooltip';
  import { modelRows } from '../lib/domain/modelBreakdown';
  let {
    summary,
    lang,
    trend,
    period,
  }: { summary: Summary | null; lang: Language; trend?: MetricTrend; period: Period | null } =
    $props();
  let series = $derived(metricSeries(trend, period));
  let costSeries = $derived(
    series.map((row) => ({ date: row.date, end: row.end, value: row.cost })),
  );
  let tokenSeries = $derived(
    series.map((row) => ({ date: row.date, end: row.end, value: row.tokens })),
  );
  let t = $derived(words(lang));
  let usage = $derived(summary?.usage);
  let showModels = $state(false);
  let modelsLayout = $state(false);
  let visiblePanel = $state<'overview' | 'models' | null>('overview');
  let reducedMotion = $state(false);
  let cardWidth = $state(400);
  let cardHeight = $state(160);
  let costToggle: HTMLButtonElement;
  const modelsId = $props.id();
  const borderMaskId = `${modelsId}-border-mask`;
  let models = $derived(summary?.model_breakdown ?? []);
  let borderSegments = $derived.by(() => {
    const rows = modelRows(models, models.some(row => (row.estimated_cost_usd ?? 0) > 0) ? 'cost' : 'tokens');
    let offset = 0;
    return rows.filter(row => row.share > 0).map(row => {
      const segment = { ...row, offset: -offset * 100 };
      offset += row.share;
      return segment;
    });
  });
  let toggleLabel = $derived(lang === 'zh'
    ? showModels ? '返回用量概览' : '按模型查看用量与费用'
    : showModels ? 'Back to usage overview' : 'Usage and cost by model');
  onMount(() => {
    const preference = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => reducedMotion = preference.matches;
    update();
    preference.addEventListener('change', update);
    return () => preference.removeEventListener('change', update);
  });
  $effect(() => { if (!models.length) showModels = false; });
  $effect(() => {
    if (visiblePanel !== null && visiblePanel !== (showModels ? 'models' : 'overview')) {
      visiblePanel = null;
    }
  });
  function finishPanelExit() {
    modelsLayout = showModels;
    visiblePanel = showModels ? 'models' : 'overview';
  }
  function escapeModels(event: KeyboardEvent) {
    if (event.key === 'Escape' && showModels && !document.querySelector('dialog[open], [popover]:popover-open')) {
      showModels = false;
      costToggle.focus({ preventScroll: true });
    }
  }
  let hasRoles = $derived(
    typeof summary?.main_count === 'number' &&
      typeof summary?.agent_count === 'number' &&
      summary.main_count + summary.agent_count === summary.session_count,
  );
  let mainCount = $derived(hasRoles ? summary!.main_count! : null);
  let agentCount = $derived(hasRoles ? summary!.agent_count! : null);
  let agentPercent = $derived(summary?.session_count && agentCount !== null ? agentCount / summary.session_count * 100 : null);
  let hit = $derived(usage ? cacheHitPercent(usage) : null);
  let cost = $derived(
    summary?.model_breakdown?.length
      ? summary.model_breakdown.some(row => row.estimated_cost_usd !== null)
        ? summary.model_breakdown.reduce((total, row) => total + (row.estimated_cost_usd ?? 0), 0)
        : null
      : summary && (!summary.session_count || summary.estimated_cost_known_count)
        ? summary.estimated_cost_usd : null,
  );
  const percent = (value: number | null) => (value === null ? '—' : `${number(value, lang)}%`);
  let roleTooltip = $derived({
    title: t.sessionComposition,
    rows: [
      {
        label: t.mainSessions,
        value: `${number(mainCount, lang)} · ${percent(mainCount !== null && summary?.session_count ? (mainCount / summary.session_count) * 100 : null)}`,
      },
      {
        label: t.agentLabel,
        value: `${number(agentCount, lang)} · ${percent(agentCount !== null && summary?.session_count ? (agentCount / summary.session_count) * 100 : null)}`,
      },
    ],
  });
  let cacheTooltip = $derived({
    title: `${t.cache} · ${percent(hit)}`,
    rows: [
      { label: t.cached, value: `${number(usage?.cached_input_tokens, lang)} tokens` },
      { label: t.input, value: `${number(usage?.input_tokens, lang)} tokens` },
    ],
  });
</script>

<svelte:window onkeydown={escapeModels} />

<section class="metrics" class:show-models={modelsLayout} style:--cost-card-height={`${cardHeight + 2}px`} aria-label={t.overview}>
  <article class="cost-card" class:interactive={models.length > 0} class:is-models={showModels}
    bind:clientWidth={cardWidth} bind:clientHeight={cardHeight}>
    <div class="metric-label">{t.cost}</div>
    <div class="metric-cost-value">
      <strong
        data-tooltip={summary ? money(cost, lang) : undefined}
        style:--value-length={cost === null ? 8 : displayMoney(cost).length}
      >
        <span class="cost-text">{#if summary && cost === null}{t.unknown}{:else}<AnimatedValue
            value={cost}
            format={displayMoney}
          />{/if}</span>
      </strong>
    </div>
    <div class="cost-spark" inert>
      <MetricSpark
        values={costSeries}
        label={t.costTrend}
        intervalLabel={t.intervalCost}
        startLabel={t.rangeStart}
        format={displayMoney}
      />
    </div>
    {#if trend && !trend.complete}<small class="metric-chart-caption"
        ><span>{t.partialTrend}</span></small
      >{/if}
    {#if summary && summary.estimated_cost_known_count < summary.session_count}<small
        class="estimate-note"
        data-tooltip={t.unknownHint}>{t.partial}</small
      >{/if}
    <svg class="model-border" viewBox={`0 0 ${cardWidth} ${cardHeight}`} preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <mask id={borderMaskId} maskUnits="userSpaceOnUse" maskContentUnits="userSpaceOnUse"
          x="-8" y="-8" width={cardWidth + 16} height={cardHeight + 16}>
          <rect x="-8" y="-8" width={cardWidth + 16} height={cardHeight + 16} fill="white" />
          <rect class="border-hole" x="1.5" y="1.5" width={Math.max(0, cardWidth - 3)} height={Math.max(0, cardHeight - 3)} rx="14" fill="black" />
        </mask>
      </defs>
      <g class="border-ink" mask={`url(#${borderMaskId})`}>
        {#each borderSegments as segment (segment.model)}
          <rect class="border-segment" x="1.5" y="1.5" width={Math.max(0, cardWidth - 3)} height={Math.max(0, cardHeight - 3)} rx="14"
            pathLength="100" stroke={segment.color} stroke-dasharray={`${segment.share * 100} ${100 - segment.share * 100}`}
            style:--segment-offset={segment.offset} />
        {/each}
      </g>
    </svg>
    <button bind:this={costToggle} class="cost-toggle" type="button" aria-label={toggleLabel}
      aria-expanded={showModels} aria-pressed={showModels} aria-controls={models.length > 6 ? `${modelsId} ${modelsId}-more` : modelsId} data-tooltip=""
      disabled={!models.length} onclick={() => showModels = !showModels}></button>
  </article>
  {#if visiblePanel === 'models'}
    <div id={modelsId} class="model-panel" role="region" aria-label={lang === 'zh' ? '模型用量与费用' : 'Model usage and cost'}
      in:fly={{ x: 20, duration: reducedMotion ? 0 : 260, easing: cubicOut }}
      out:fly={{ x: 14, duration: reducedMotion ? 0 : 140, easing: cubicOut }}
      onoutroend={finishPanelExit}>
      <ModelBreakdown {summary} {lang} metric="cost" limit={6} />
    </div>
    {#if models.length > 6}
      <div id={`${modelsId}-more`} class="model-overflow"
        in:fly={{ x: 20, duration: reducedMotion ? 0 : 260, easing: cubicOut }}
        out:fly={{ x: 14, duration: reducedMotion ? 0 : 140, easing: cubicOut }}>
        <ModelBreakdown {summary} {lang} metric="cost" start={6} wide />
      </div>
    {/if}
  {:else if visiblePanel === 'overview'}
  <article class="token-card"
    in:fly={{ x: -12, duration: reducedMotion ? 0 : 230, easing: cubicOut }}
    out:fly={{ x: 12, duration: reducedMotion ? 0 : 150, easing: cubicOut }}>
    <details class="metric-breakdown" use:dismissDetails>
      <summary class="metric-label"
        >{t.tokens}<Icon name="down" size={13} /></summary
      >
      <div class="breakdown-values">
        {#each [['input', usage?.input_tokens], ['cached', usage?.cached_input_tokens], ['cacheWrite', usage?.cache_write_tokens], ['output', usage?.output_tokens], ['reasoning', usage?.reasoning_output_tokens]] as [key, value]}<span
            >{t[key as keyof typeof t]}<b
              ><AnimatedValue
                value={summary ? ((value as number) ?? 0) : null}
                format={(value) => number(Math.round(value), lang)}
              /></b
            ></span
          >{/each}
      </div>
    </details>
    <div class="metric-value">
      <strong
        data-tooltip={number(usage?.total_tokens, lang)}
        style:--value-length={compactNumber(usage?.total_tokens).length}
        ><AnimatedValue
          value={summary ? (usage?.total_tokens ?? 0) : null}
          format={compactNumber}
        /></strong
      ><span class="metric-unit">tokens</span>
    </div>
    <DailyUsage
      values={tokenSeries}
      label={trend?.granularity === 'hour' ? t.hourlyTokens : series.some((row) => row.date !== row.end) ? t.intervalTokens : t.dailyTokens}
      {lang}
    />
  </article>
  <article class="sessions-card"
    in:fly={{ x: -12, duration: reducedMotion ? 0 : 230, delay: reducedMotion ? 0 : 25, easing: cubicOut }}
    out:fly={{ x: 12, duration: reducedMotion ? 0 : 175, easing: cubicOut }}>
    <div class="metric-label">{t.sessions}</div>
    <div class="session-value">
      <strong
        style:--value-length={number(summary?.session_count, lang).length}
        ><AnimatedValue
          value={summary?.session_count ?? null}
          format={(value) => number(Math.round(value), lang)}
        /></strong
      >
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users inspect chart values on focus.) -->
      <div
        class="session-chart"
        role="img"
        tabindex="0"
        use:metricTooltip={roleTooltip}
        aria-label={`${t.sessionComposition}: ${t.mainSessions} ${mainCount ?? '—'}, ${t.agentLabel} ${agentCount ?? '—'}`}
      >
        <MetricRing value={agentPercent} className="session-ring" segmented />
      </div>
    </div>
    <div class="session-composition">
      <div class="session-role-values">
        <div class="session-role">
          <strong class="role-value" data-role="main"
            ><AnimatedValue
              value={mainCount}
              format={(value) => number(Math.round(value), lang)}
            /></strong
          ><span><i class="main-role"></i>{t.mainSessions}</span>
        </div>
        <div class="session-role">
          <strong class="role-value" data-role="agent"
            ><AnimatedValue
              value={agentCount}
              format={(value) => number(Math.round(value), lang)}
            /></strong
          ><span><i class="agent-role"></i>{t.agentLabel}</span>
        </div>
      </div>
    </div>
  </article>
  <article class="cache-card" data-cache-tone={cacheHitTone(hit)}
    in:fly={{ x: -12, duration: reducedMotion ? 0 : 230, delay: reducedMotion ? 0 : 50, easing: cubicOut }}
    out:fly={{ x: 12, duration: reducedMotion ? 0 : 200, easing: cubicOut }}
    onoutroend={finishPanelExit}>
    <div class="metric-label">{t.cache}</div>
    <div class="cache-value">
      <strong style:--value-length={number(hit, lang).length + 1}
        ><AnimatedValue value={hit} format={(value) => number(value, lang) + '%'} /></strong
      >
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users inspect chart values on focus.) -->
      <div
        class="cache-chart"
        role="img"
        tabindex="0"
        aria-label={`${t.cache}: ${percent(hit)}`}
        use:metricTooltip={cacheTooltip}
      >
        <MetricRing value={hit} className="cache-ring" />
      </div>
    </div>
    <div class="cache-counts">
      {t.cacheCounts}
      <b
        ><AnimatedValue
          value={summary ? (usage?.cached_input_tokens ?? 0) : null}
          format={compactNumber}
        /></b
      >
      / {t.input}
      <b
        ><AnimatedValue
          value={summary ? (usage?.input_tokens ?? 0) : null}
          format={compactNumber}
        /></b
      >
    </div>
  </article>
  {/if}
</section>

<style>
  .cost-text { display: inline-block; }
  .metrics .cost-card { grid-column: 1; grid-row: 1; border-color: transparent; }
  .metrics.show-models { --model-card-height: max(84px, calc((var(--cost-card-height) - 10px) / 2)); grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 10px; }
  .metrics.show-models .cost-card { grid-column: 1 / span 2; }
  .token-card { grid-column: 2; grid-row: 1; }
  .sessions-card { grid-column: 3; grid-row: 1; }
  .cache-card { grid-column: 4; grid-row: 1; }
  .metrics > article:not(.cost-card) { animation: none; }
  .metrics .model-panel { grid-column: 3 / -1; grid-row: 1; min-width: 0; min-height: 0; align-self: start; }
  .model-overflow { grid-column: 1 / -1; grid-row: 2; min-width: 0; }
  .cost-toggle { position: absolute; inset: 0; z-index: 3; width: 100%; height: 100%; min-height: 0; padding: 0; border: 0; border-radius: inherit; background: transparent; box-shadow: none; transform: none; transition: none; }
  .cost-toggle:hover, .cost-toggle:active { background: transparent; transform: none; }
  .cost-toggle:focus-visible { outline: 2px solid var(--accent); outline-offset: 4px; }
  .model-border { position: absolute; inset: -1px; z-index: 4; width: calc(100% + 2px); height: calc(100% + 2px); pointer-events: none; overflow: visible; opacity: 0; transition: opacity 180ms ease; }
  /* The fixed mask removes the inner half of the stroke, so only its outer edge breathes. */
  .border-ink { stroke-width: 4; animation: border-breathe 1.65s ease-in-out infinite; animation-play-state: paused; }
  .border-segment { fill: none; stroke-linecap: butt; stroke-dashoffset: var(--segment-offset); animation: border-flow 5s linear infinite; animation-play-state: paused; }
  .cost-card.interactive:hover .model-border, .cost-card.is-models .model-border { opacity: 1; }
  .cost-card.interactive:hover .border-ink, .cost-card.is-models .border-ink,
  .cost-card.interactive:hover .border-segment, .cost-card.is-models .border-segment { animation-play-state: running; }
  @keyframes border-flow { to { stroke-dashoffset: calc(var(--segment-offset) - 100); } }
  @keyframes border-breathe {
    0%, 100% { stroke-width: 4; }
    50% { stroke-width: 7; }
  }
  @media (max-width: 1100px) {
    .metrics.show-models { --model-card-height: 84px; grid-template-columns: 1.4fr 1fr; }
    .sessions-card { grid-column: 1; grid-row: 2; }
    .cache-card { grid-column: 2; grid-row: 2; }
    .metrics.show-models .cost-card { grid-column: 1 / -1; }
    .metrics .model-panel { grid-column: 1 / -1; grid-row: 2; }
    .model-overflow { grid-row: 3; }
  }
  @media (max-width: 600px) {
    .metrics.show-models { gap: 8px; }
  }
  @media (prefers-reduced-motion: reduce) {
    .model-border { transition: none; }
    .border-ink, .border-segment { animation: none; }
  }
</style>
