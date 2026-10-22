import { timezoneCities } from './data/timezoneCities';
import type { Language } from './ui';

export function timezoneOption(zone: string, lang: Language, now = new Date()): readonly [string, string] {
  const canonical = new Intl.DateTimeFormat('en', { timeZone: zone }).resolvedOptions().timeZone;
  const city = timezoneCities[zone]?.[lang] ?? timezoneCities[canonical]?.[lang];
  const name = canonical === 'UTC'
    ? lang === 'zh' ? '协调世界时' : 'Coordinated Universal Time'
    : city ?? (lang === 'en'
      ? canonical.split('/').at(-1)!.replaceAll('_', ' ')
      : new Intl.DateTimeFormat('zh-CN', { timeZone: zone, timeZoneName: 'longGeneric' })
        .formatToParts(now).find(part => part.type === 'timeZoneName')!.value);
  const offset = new Intl.DateTimeFormat('en', { timeZone: zone, timeZoneName: 'longOffset' })
    .formatToParts(now).find(part => part.type === 'timeZoneName')!.value
    .replace('GMT', 'UTC');
  return [zone, `${name} (${offset === 'UTC' ? 'UTC+00:00' : offset})`];
}
