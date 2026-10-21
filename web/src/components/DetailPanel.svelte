<script lang="ts">
  import { untrack } from 'svelte';
  import type { DashboardData, DetailBundle, Events } from '../lib/data/types';
  import { cancelled } from '../lib/data/http';
  import { words, number, money, dateTime, models, type Language } from '../lib/domain/ui';
  import { chart } from '../lib/domain/chart';
  import Pager from './Pager.svelte';
  let {
    bundle,
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

<aside class="detail-panel" aria-label={t.details} aria-busy={loading}>
  <header class="detail-heading">
    <div>
      <span class="eyebrow">{t.details}</span>
      <h2>{d?.title || t.loading}</h2>
    </div>
    <button class="icon-button" aria-label={t.close} onclick={close}>×</button>
  </header>
  {#if loading && !bundle}<p role="status">{t.loading}</p>{/if}{#if error}<p
      class="error"
      role="alert"
    >
      {error} <button onclick={retry}>{t.retry}</button>
    </p>{/if}
  {#if d && bundle}<div class="detail-scroll">
      <p class="muted">
        {d.environment} · {models(d).join(', ')} · {d.service_tier || 'standard'}{d.effort
          ? ' · ' + d.effort
          : ''}
      </p>
      <div class="detail-metrics">
        <div>
          <small>Tokens</small><strong>{number(d.total_token_usage?.total_tokens, lang)}</strong>
        </div>
        <div><small>{t.cost}</small><strong>{money(d.estimated_cost_usd, lang)}</strong></div>
      </div>
      {#if d.inherited_token_event_count > 0}<p class="notice">
          {t.inherited}: {number(d.inherited_token_usage?.total_tokens, lang)} tokens
        </p>{/if}{#if d.is_subagent && !d.fork_usage_resolved}<p class="notice">
          {t.unresolved}
        </p>{/if}
      <figure>
        <figcaption>{t.curve} <small>{t.fullCurve}</small></figcaption>
        <canvas use:chart={bundle.chart} aria-label={t.curve}></canvas>
      </figure>
      <section>
        <h3>{t.events} <span class="muted">{number(page?.total, lang)}</span></h3>
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
                    ><td class="numeric">{number(row.total_token_usage?.input_tokens, lang)}</td><td
                      class="numeric">{number(row.total_token_usage?.output_tokens, lang)}</td
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
      </section>
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
        <summary>{t.tools}</summary>{#each Object.entries(d.tool_counts || {}) as [name, count]}<div
            class="price-row"
          >
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
          <dt>CLI</dt>
          <dd>{d.cli_version || '—'}</dd>
          <dt>Parse errors</dt>
          <dd>{d.parse_errors || 0}</dd>
        </dl>
      </details>
    </div>{/if}
</aside>
