import type { SpeedTotals, TokenEvent } from '../data/types';
import type { Language } from './ui';

export function estimatedTps(speed?: SpeedTotals | null): number | null {
  if (!speed || !Number.isFinite(speed.output_tokens) || !Number.isFinite(speed.duration_ms)
    || speed.output_tokens <= 0 || speed.duration_ms <= 0 || !(speed.request_count > 0)) return null;
  const value = speed.output_tokens * 1000 / speed.duration_ms;
  return Number.isFinite(value) && value > 0 ? value : null;
}

export function formatTps(speed: SpeedTotals | null | undefined, lang: Language, compact = false): string {
  const value = estimatedTps(speed);
  return value === null ? '—' : `${compact ? '' : '≈'}${new Intl.NumberFormat(lang === 'zh' ? 'zh-CN' : 'en-US', value < 0.1 ? { maximumSignificantDigits: 2 } : { maximumFractionDigits: 1 }).format(value)}${compact ? '' : ' tok/s'}`;
}

/** Uses normalized request output, with the same timing eligibility as the backend. */
export function eventSpeed(event: TokenEvent): SpeedTotals | null {
  const output = event.last_token_usage?.output_tokens;
  const first = event.generation_duration_ms != null;
  const duration = first ? event.generation_duration_ms : event.request_duration_ms;
  if (output == null || !Number.isFinite(output) || output <= 0 || duration == null
    || !Number.isFinite(duration) || duration < (first ? 100 : 1000) || duration > 3600000) return null;
  return { output_tokens: output, duration_ms: duration, request_count: 1, first_token_request_count: first ? 1 : 0 };
}

export function usageSpeedHint(speed: SpeedTotals | null | undefined, lang: Language): string {
  return formatTps(speed, lang).replace(/^≈/, '');
}

export function speedHint(speed: SpeedTotals | null | undefined, lang: Language): string {
  const count = speed?.request_count ?? 0;
  const first = speed?.first_token_request_count ?? 0;
  return lang === 'zh'
    ? `估算 TPS：有效请求的总输出 ÷ 总计时时长，推理 token 已包含在输出中。优先从首 token 计时，缺失时按完整请求耗时估算；尽量排除工具执行及用户等待。有效请求 ${count} 条，其中 ${first} 条有首 token 计时，${count - first} 条包含首 token 等待。生成窗口须 ≥0.1 秒，回退耗时须 ≥1 秒，均不超过 1 小时；不限制输出 token 数。`
    : `Estimated TPS: total output / total timed duration, including reasoning tokens. Uses first-token timing when available, otherwise full request duration; excludes tool execution and user idle time where possible. ${count} eligible requests: ${first} timed from first token, ${count - first} including first-token wait. Generation window ≥0.1s, fallback duration ≥1s; both ≤1h. No output-token minimum.`;
}
