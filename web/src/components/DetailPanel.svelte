<script lang="ts">
  import { untrack, onMount, tick } from 'svelte';
  import type { DashboardData, DetailBundle, Events } from '../lib/data/types';
  import { cancelled } from '../lib/data/http';
  import { words, number, money, dateTime, models, type Language } from '../lib/domain/ui';
  import UsageChart from './UsageChart.svelte';
  import Icon from './Icon.svelte';
  import AnimatedValue from './AnimatedValue.svelte';
  import { compactNumber, displayMoney, costSegments } from '../lib/domain/presentation';
  import Pager from './Pager.svelte';
  let {
    bundle,
    projectView,
    loading,
    error,
    lang,
    zone,
    data,
    close,
    retry,
    onerror,
  }: {
    bundle: DetailBundle | null;
    projectView: boolean;
    loading: boolean;
    error: string;
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
  let copied = $state(false);
  let d = $derived(bundle?.detail);
  let panel: HTMLElement;
  let mobile = $state(false);
  let segments = $derived(d ? costSegments(d) : []);
  let segmentTotal = $derived(segments.reduce((sum, part) => sum + part.value, 0));
  onMount(() => {
    const media = matchMedia('(max-width: 1100px)');
    const update = () => {
      const entering = media.matches && !mobile;
      mobile = media.matches;
      if (entering) void tick().then(() => panel?.focus());
    };
    update();
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  });
  function keyboard(event: KeyboardEvent) {
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
    if (!mobile) return;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = overflow;
    };
  });
  $effect(() => {
    const next = bundle;
    untrack(() => {
      const offset = page?.id === next?.detail.uid ? page?.offset || 0 : 0;
      page = next?.events || null;
      copied = false;
      if (next && offset) void move('', offset);
    });
  });
  async function move(cursor: string, offset?: number) {
    if (!d) return;
    const id = d.uid,
      revision = d.revision;
    paging = true;
    try {
      const result = await data.events(id, revision, cursor, offset);
      if (d?.uid === id && d.revision === revision) page = result;
    } catch (e) {
      if (!cancelled(e)) onerror(e);
    } finally {
      paging = false;
    }
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
      <h2 title={d?.title}>{d?.title || t.loading}</h2>
      {#if d && projectView}<span class="role-tag">{d.is_subagent ? t.agentLabel : t.ownUsage}</span
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
      {error} <button onclick={retry}>{t.retry}</button>
    </p>{/if}
  {#if d && bundle}<div class="detail-scroll">
      <div class="detail-tags">
        <span>{t.projectLabel}: {d.project}</span><i></i><span
          >{t.modelLabel}: {models(d).join(', ')}</span
        >
      </div>
      <div class="detail-metrics">
        <div class="detail-cost">
          <small>{t.cost}</small><strong
            title={money(d.estimated_cost_usd, lang)}
            style:--value-length={d.estimated_cost_usd == null
              ? 8
              : displayMoney(d.estimated_cost_usd).length}
            >{#if d.estimated_cost_usd == null}{t.unknown}{:else}<AnimatedValue
                value={d.estimated_cost_usd}
                format={displayMoney}
              />{/if}</strong
          >
        </div>
        <div class="detail-tokens">
          <small>{t.tokens}</small><strong title={number(d.total_token_usage?.total_tokens, lang)}
            >{compactNumber(d.total_token_usage?.total_tokens)}</strong
          ><span class="detail-token-unit">tokens</span>
        </div>
      </div>
      {#if projectView}<p class="own-usage-hint">{d.is_subagent ? t.agentHint : t.ownHint}</p>{/if}
      {#if d.inherited_token_event_count > 0}<p class="notice">
          {t.inherited}: {number(d.inherited_token_usage?.total_tokens, lang)} tokens
        </p>{/if}
      {#if d.is_subagent && !d.fork_usage_resolved}<p class="notice">{t.unresolved}</p>{/if}
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
            {#each segments as part}<div>
                <span class="cost-label"><i style:background={part.color}></i>{t[part.key]}</span
                ><strong title={money(part.value, lang)}>{displayMoney(part.value)}</strong><small
                  >{segmentTotal
                    ? Math.round((part.value / segmentTotal) * 100) + '%'
                    : '0%'}</small
                >
              </div>{/each}
          </div>
          {#if d.estimated_cost_breakdown_usd?.reasoning_output_tokens}<p class="composition-hint">
              {t.reasoningIncluded}
            </p>{/if}
        {:else}<p class="muted">{t.unknownHint}</p>{/if}
        <p class="cost-disclaimer"><Icon name="info" size={15} />{t.estimatedHint}</p>
      </section>
      <details class="events-section">
        <summary
          ><Icon name="message" size={23} /><span>{t.events}</span><span class="detail-count"
            >{number(page?.total, lang)}</span
          ><Icon name="chevron" size={17} /></summary
        >
        <div class="event-content">
          {#if page?.events.length}<div class="table-scroll event-scroll">
              <table>
                <thead
                  ><tr
                    ><th>{t.updated}</th><th>{t.model}</th><th class="numeric">{t.input}</th><th
                      class="numeric">{t.output}</th
                    ><th class="numeric">Tokens</th></tr
                  ></thead
                ><tbody
                  >{#each page.events as row, i}<tr
                      ><td title={row.timestamp || ''}>{dateTime(row.timestamp, zone, lang)}</td><td
                        >{row.model || d.model}<small>{row.service_tier || 'standard'}</small></td
                      ><td class="numeric">{number(row.total_token_usage?.input_tokens, lang)}</td
                      ><td class="numeric">{number(row.total_token_usage?.output_tokens, lang)}</td
                      ><td class="numeric">{number(row.total_token_usage?.total_tokens, lang)}</td
                      ></tr
                    >{/each}</tbody
                >
              </table>
            </div>{:else}<p class="muted">{t.noEvents}</p>{/if}{#if page}<Pager
              offset={page.offset}
              total={page.total}
              limit={page.limit}
              next={page.next_cursor}
              previous={page.previous_cursor}
              disabled={paging}
              change={move}
              {lang}
            />{/if}
        </div>
      </details>

      <details class="more-information">
        <summary
          ><Icon name="info" size={23} /><span>{t.moreInfo}</span><Icon
            name="chevron"
            size={17}
          /></summary
        >
        <div class="more-information-content">
          {#if d.first_user_prompt}<details>
              <summary>{t.prompt}</summary>
              <pre>{d.first_user_prompt}</pre>
            </details>{/if}{#if d.last_agent_preview}<details>
              <summary>{t.response}</summary>
              <pre>{d.last_agent_preview}</pre>
            </details>{/if}
          <details>
            <summary>{t.prices}</summary>
            <p class="muted">{t.estimatedHint} {t.unknownHint}</p>
            {#each d.applied_price_segments || [] as segment}<div class="price-row">
                <span>{segment.model} · {segment.service_tier || 'standard'}</span><b
                  >{money(segment.estimated_cost_usd, lang)}</b
                >
              </div>{/each}
          </details>
          <details>
            <summary>{t.tools}</summary
            >{#each Object.entries(d.tool_counts || {}) as [name, count]}<div class="price-row">
                <span>{name}</span><b>{number(count, lang)}</b>
              </div>{/each}
          </details>
          <details>
            <summary>{t.metadata}</summary>
            <dl>
              <dt>Session ID</dt>
              <dd>{d.session_id}</dd>
              <dt>{t.path}</dt>
              <dd>
                {d.path}
                <button
                  onclick={async () => {
                    await navigator.clipboard.writeText(d.path);
                    copied = true;
                  }}>{copied ? t.copied : t.copy}</button
                >
              </dd>
              <dt>{t.projects}</dt>
              <dd>{d.cwd}</dd>
              <dt>{t.updated}</dt>
              <dd>{dateTime(d.end_at, zone, lang)}</dd>
              <dt>{t.duration}</dt>
              <dd>{number(d.duration_ms_total, lang)} ms</dd>
              <dt>{t.turns}</dt>
              <dd>{number(d.completed_turn_count, lang)} / {number(d.turn_count, lang)}</dd>
              <dt>{t.environment}</dt>
              <dd>{d.environment}</dd>
              <dt>{t.model}</dt>
              <dd>{models(d).join(', ')} · {d.service_tier} · {d.effort}</dd>
              <dt>CLI</dt>
              <dd>{d.cli_version || '—'}</dd>
              <dt>Parse errors</dt>
              <dd>{d.parse_errors || 0}</dd>
            </dl>
          </details>
        </div>
      </details>
    </div>{/if}
</aside>
