import type { Detail, Usage } from '../data/types';
import { models, number, words, zero, type Language } from './ui';
import { displayMoney, serviceTier, serviceTierLabel } from './presentation';
import type { MetricTooltipData, ModelTagData } from './metricTooltip';

type Category = 'input' | 'cached' | 'cacheWrite' | 'output' | 'reasoning';
const categories: { key: Category; token: keyof Usage; rate: string; color: string }[] = [
  { key: 'input', token: 'input_tokens', rate: 'input', color: 'var(--cost-input)' },
  { key: 'cached', token: 'cached_input_tokens', rate: 'cached_input', color: 'var(--cost-cache)' },
  { key: 'cacheWrite', token: 'cache_write_tokens', rate: 'cache_write_input', color: 'var(--cost-write)' },
  { key: 'output', token: 'output_tokens', rate: 'output', color: 'var(--cost-output)' },
  { key: 'reasoning', token: 'reasoning_output_tokens', rate: 'output', color: 'var(--cost-output)' },
];

function tokenCount(usage: Partial<Usage>, key: keyof Usage) {
  return Math.max(0, key === 'input_tokens'
    ? (usage.input_tokens ?? 0) - (usage.cached_input_tokens ?? 0)
    : usage[key] ?? 0);
}

export function usageBreakdown(detail: Detail) {
  return categories.filter(part => part.key !== 'cacheWrite'
    || tokenCount(detail.total_token_usage, part.token) > 0
    || !!detail.estimated_cost_breakdown_usd?.cache_write_tokens).map(part => ({
      ...part,
      nested: part.key === 'reasoning',
      tokens: tokenCount(detail.total_token_usage, part.token),
      cost: detail.estimated_cost_breakdown_usd?.[part.token] ?? null,
    }));
}

/** Show every category together; recorded rates already include service-tier multipliers. */
export function usagePriceTooltip(detail: Detail, lang: Language, zone = 'UTC'): MetricTooltipData {
  const t = words(lang);
  const parts = usageBreakdown(detail);
  type Group = { caption: string; modelTags: ModelTagData; period: string; values: { tokens: number; rate: number | null; cost: number | null }[] };
  const groups = new Map<string, Group>();
  const tagsFor = (model: string, effort: string, tier: string, longContext = false): ModelTagData => ({
    model, effort, service_tier: serviceTier(tier === 'default' ? 'standard' : tier), longContext, lang,
  });
  const captionFor = (tags: ModelTagData) => [
    tags.model, tags.effort || t.unknown,
    ['fast', 'ultrafast'].includes(tags.service_tier) ? serviceTierLabel(tags.service_tier) : '',
    tags.longContext ? t.longContext : '',
  ].filter(Boolean).join(' · ');
  for (const segment of detail.applied_price_segments ?? []) {
    const values = parts.map(part => {
      const tokens = tokenCount(segment.usage ?? {}, part.token);
      const prices = segment.prices as Record<string, number | null> | undefined;
      const raw = prices?.[part.rate] ?? (part.key === 'cacheWrite' ? prices?.input : null);
      const rate = segment.price_known !== false && typeof raw === 'number' && Number.isFinite(raw) ? raw : null;
      const costs = segment.estimated_cost_breakdown_usd as Partial<Usage> | undefined;
      const cost = costs?.[part.token] ?? (rate === null ? null : tokens * rate / 1e6);
      return { tokens, rate, cost };
    });
    const modelTags = tagsFor(segment.model || detail.model,
      typeof segment.effort === 'string' ? segment.effort : detail.effort,
      segment.service_tier, segment.context_tier === 'long');
    const caption = captionFor(modelTags);
    const key = JSON.stringify([modelTags, segment.context_tier, segment.effective_at ?? null, segment.effective_until ?? null, values.map(value => value.rate)]);
    const previous = groups.get(key);
    if (previous) values.forEach((value, i) => {
      const before = previous.values[i];
      before.tokens += value.tokens;
      before.cost = before.cost === null || value.cost === null ? null : before.cost + value.cost;
    });
    else groups.set(key, { caption, modelTags, values, period: pricePeriod(segment, lang, zone) });
  }
  if (!groups.size) groups.set('', {
    caption: captionFor(tagsFor(models(detail).join(', '), detail.effort, detail.service_tier)),
    modelTags: tagsFor(models(detail).join(', '), detail.effort, detail.service_tier),
    period: '',
    values: parts.map(part => ({ tokens: part.tokens, cost: part.cost, rate: null })),
  });
  // A single price group can use the exact server totals, including rounded estimates.
  if (groups.size === 1) [...groups.values()][0].values.forEach((value, i) => {
    value.tokens = parts[i].tokens;
    if (parts[i].cost !== null) value.cost = parts[i].cost;
  });
  return {
    title: '', rows: [], className: 'metric-tooltip price-tooltip',
    tables: [...groups.values()].map(group => ({
      caption: group.caption,
      modelTags: group.modelTags,
      period: group.period,
      columns: [lang === 'zh' ? '类别' : 'Category', 'Tokens', lang === 'zh' ? '单价 / 1M' : 'Rate / 1M', t.sortCost],
      rows: parts.map((part, i) => ({
        label: part.nested ? t.reasoningSubset : t[part.key], color: part.color, nested: part.nested,
        values: [number(group.values[i].tokens, lang), group.values[i].rate === null ? t.unknown : displayMoney(group.values[i].rate!), group.values[i].cost === null ? t.unknown : displayMoney(group.values[i].cost!)],
      })),
    })),
    note: t.reasoningIncluded,
  };
}

/** Price validity comes from the schedule, independently of the session's activity dates. */
function pricePeriod(segment: Detail['applied_price_segments'][number], lang: Language, zone: string) {
  const format = (value: string | null | undefined) => {
    if (!value || !Number.isFinite(Date.parse(value))) return '';
    const parts = new Intl.DateTimeFormat('en-GB', {
      timeZone: zone, year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
    }).formatToParts(new Date(value));
    const get = (key: Intl.DateTimeFormatPartTypes) => parts.find(part => part.type === key)?.value;
    return `${get('year')}/${get('month')}/${get('day')} ${get('hour')}:${get('minute')}`;
  };
  const start = format(segment.effective_at), end = format(segment.effective_until);
  if (!start && !end) return '';
  const range = start && end ? `${start} – ${end}`
    : start ? (lang === 'zh' ? `${start} 起` : `From ${start}`)
    : (lang === 'zh' ? `${end} 前` : `Before ${end}`);
  return range;
}

export function segmentPriceTooltip(detail: Detail, segment: Detail['applied_price_segments'][number], lang: Language, zone = 'UTC') {
  const model = segment.model || detail.model;
  const tooltip = usagePriceTooltip({
    ...detail,
    model,
    models: [model],
    effort: typeof segment.effort === 'string' ? segment.effort : detail.effort,
    service_tier: segment.service_tier,
    total_token_usage: segment.usage ?? zero(),
    estimated_cost_usd: segment.estimated_cost_usd,
    estimated_cost_breakdown_usd: segment.estimated_cost_breakdown_usd as Partial<Usage> | undefined ?? null,
    applied_price_segments: [segment],
  }, lang, zone);
  return { ...tooltip, followPointer: false };
}
