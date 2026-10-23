<script lang="ts">
  import type { DashboardData, Remote, LocalDevice } from '../lib/data/types';
  import { words, type Language } from '../lib/domain/ui';
  import { timezoneOption } from '../lib/domain/timezones';
  import Dropdown from './Dropdown.svelte';
  import RemoteManager from './RemoteManager.svelte';

  let { data, remotes, localDevice, lang, timezone, languageChanged, timezoneChanged, changed }: {
    data: DashboardData;
    remotes: Remote[];
    localDevice?: LocalDevice;
    lang: Language;
    timezone: string;
    languageChanged: (value: Language) => void;
    timezoneChanged: (value: string) => void;
    changed: () => void;
  } = $props();
  let t = $derived(words(lang));
  const supportedTimezones = Intl.supportedValuesOf('timeZone');
  let timezoneOptions = $derived(
    [...new Set(['UTC', timezone, ...supportedTimezones])]
      .sort()
      .map((value) => timezoneOption(value, lang)),
  );
</script>

<header class="page-header settings-header"><h1 tabindex="-1">{t.settings}</h1></header>
<div class="settings-scroll">
  <div class="settings-content">
    <section class="settings-card" aria-labelledby="general-title">
      <h2 id="general-title">{t.general}</h2>
      <div class="setting-row">
        <span>{t.language}</span>
        <div class="setting-control">
          <Dropdown label={t.language} value={lang} options={[["zh", "中文"], ["en", "English"]]} change={(value) => languageChanged(value as Language)} />
        </div>
      </div>
      <div class="setting-row">
        <span>{t.timezone}</span>
        <div class="setting-control">
          <Dropdown label={t.timezone} value={timezone} options={timezoneOptions} change={timezoneChanged}
            searchLabel={lang === 'zh' ? '搜索时区' : 'Search time zones'}
            emptyLabel={lang === 'zh' ? '没有匹配的时区' : 'No matching time zones'} />
        </div>
      </div>
    </section>
    <div class="settings-card"><RemoteManager {data} {remotes} {localDevice} {lang} {changed} /></div>
  </div>
</div>

<style>
  .settings-header { margin-bottom: 24px; }
  h1 { margin: 0; }
  h2 { margin: 0 0 12px; }
  .settings-scroll { flex: 1; min-height: 0; overflow: auto; padding: 2px 4px 24px 2px; }
  .settings-content { width: 100%; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); align-items: start; gap: 24px; }
  .settings-card { min-width: 0; padding: 28px 32px; border: 1px solid var(--border); border-radius: 16px; background: var(--panel); }
  .settings-card :global(.dropdown > summary),
  .settings-card :global(input:not([type="file"])),
  .settings-card :global(button) { height: 40px; min-height: 40px; padding: 9px 12px; border-radius: 9px; font-size: 13px; line-height: 20px; }
  .setting-row { display: grid; grid-template-columns: 88px minmax(0, 1fr); align-items: center; gap: 20px; padding: 20px 0; }
  .setting-row + .setting-row { border-top: 1px solid var(--border); }
  .setting-row:last-child { padding-bottom: 0; }
  .setting-control { min-width: 0; }
  @media (max-width: 1100px) {
    .settings-content { grid-template-columns: minmax(0, 1fr); }
    .setting-row { grid-template-columns: minmax(88px, 1fr) minmax(0, 420px); }
  }
  @media (max-width: 600px) {
    .settings-card { padding: 22px 18px; }
    .settings-content { gap: 16px; }
    .setting-row { grid-template-columns: minmax(0, 1fr); gap: 12px; }
  }
</style>
