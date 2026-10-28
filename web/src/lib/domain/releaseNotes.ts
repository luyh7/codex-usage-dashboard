import type { Language } from './ui';

export type ReleaseNoteKind = 'added' | 'fixed' | 'improved' | 'feature';
export type ReleaseNote = { kind: ReleaseNoteKind; text: string };
export type ReleaseNotes = { items: ReleaseNote[]; fallback: string };

function headingKind(heading: string): ReleaseNoteKind | null {
  const name = heading.split(/[·|]/, 1)[0].trim();
  if (/^(下载|安装与更新|安装和更新|安装|命令行|命令行与 Skill|回退|校验|downloads?|install(?:ation)?(?:\s*&\s*update)?|cli(?:\s*&\s*skill)?|checksums?|rollback)$/i.test(name)) return null;
  if (/^(修复|fix(?:es|ed)?)(?:\s|·|$)/i.test(heading)) return 'fixed';
  if (/^(新增|added|new)(?:\s|·|$)/i.test(heading)) return 'added';
  if (/^(改进|优化|improv(?:ed|ements?))(?:\s|·|$)/i.test(heading)) return 'improved';
  return 'feature';
}

function cleanInline(text: string): string {
  return text
    .replace(/!\[([^\]]*)\]\([^)]+\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/<[^>]*>/g, '')
    .replace(/[`*]/g, '')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .trim();
}

function explicitKind(zh: string, en: string): ReleaseNoteKind | null {
  if (/^(修复|修正)/.test(zh) || /^fix(?:ed)?\b/i.test(zh) || /^fix(?:ed)?\b/i.test(en)) return 'fixed';
  if (/^(新增|增加|添加)/.test(zh) || /^add(?:ed)?\b/i.test(zh) || /^add(?:ed)?\b/i.test(en)) return 'added';
  if (/^(优化|改进)/.test(zh) || /^improv(?:e|ed)\b/i.test(zh) || /^improv(?:e|ed)\b/i.test(en)) return 'improved';
  return null;
}

function withoutRepeatedLabel(text: string, kind: ReleaseNoteKind, lang: Language): string {
  const prefixes = lang === 'zh'
    ? { added: /^(新增|增加|添加)\s*/, fixed: /^(修复|修正)\s*/, improved: /^(优化|改进)\s*/, feature: /^$/ }
    : { added: /^add(?:ed)?\s+/i, fixed: /^fix(?:ed)?\s+/i, improved: /^improv(?:e|ed)\s+/i, feature: /^$/ };
  return text.replace(prefixes[kind], '') || text;
}

/** Parse the bilingual release-note list. Keep unfamiliar formats as plain text. */
export function parseReleaseNotes(source: string | null | undefined, lang: Language): ReleaseNotes {
  const raw = source?.trim() || '';
  if (!raw) return { items: [], fallback: '' };

  const items: ReleaseNote[] = [];
  let section: ReleaseNoteKind | null = 'feature';
  let bullet: string[] | null = null;
  let details = false;
  let code = false;
  let sawHeading = false;
  let preamble = false;
  let unsupported = false;
  const flush = () => {
    if (!bullet || !section) { bullet = null; return; }
    const [first, ...rest] = bullet.join(' ').split(/<br\s*\/?>/i);
    const zh = cleanInline(first), en = cleanInline(rest.join(' '));
    if (zh) {
      const kind = section === 'feature' ? explicitKind(zh, en) || 'feature' : section;
      const text = lang === 'en' && en ? en : zh;
      items.push({ kind, text: withoutRepeatedLabel(text, kind, lang) });
    }
    bullet = null;
  };

  for (const line of raw.split(/\r?\n/)) {
    if (/^\s*<details(?:\s|>)/i.test(line)) { flush(); if (section && sawHeading) unsupported = true; details = true; continue; }
    if (details) { if (/<\/details\s*>/i.test(line)) details = false; continue; }
    if (/^\s*```/.test(line)) { flush(); if (section && sawHeading) unsupported = true; code = !code; continue; }
    if (code) continue;
    const heading = line.match(/^\s*#{2,6}\s+(.+?)\s*#*\s*$/);
    if (heading) { flush(); sawHeading = true; section = headingKind(cleanInline(heading[1])); continue; }
    const match = line.match(/^\s*[-*+]\s+(.+)$/);
    if (match && section) { flush(); bullet = [match[1]]; continue; }
    if (bullet && /^\s+\S/.test(line)) { bullet.push(line.trim()); continue; }
    if (bullet && !line.trim()) { flush(); continue; }
    if (line.trim() && section && sawHeading) { flush(); unsupported = true; }
    else if (line.trim() && !sawHeading && !/^\s*#\s/.test(line)) preamble = true;
  }
  flush();
  return items.length && !unsupported && (sawHeading || !preamble) ? { items, fallback: '' } : { items: [], fallback: raw };
}
