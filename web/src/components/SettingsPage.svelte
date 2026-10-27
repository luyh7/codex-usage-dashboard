<script lang="ts">
  import type { DashboardData, Remote, LocalDevice } from '../lib/data/types';
  import { words, type Language } from '../lib/domain/ui';
  import { timezoneOption } from '../lib/domain/timezones';
  import { displaySourcePath, sourcePathKey } from '../lib/domain/logSources';
  import Dropdown from './Dropdown.svelte';
  import RemoteManager from './RemoteManager.svelte';
  import DesktopSettings from './DesktopSettings.svelte';
  import { isDesktop, desktop } from '../lib/data/desktop';
  import { preferences } from '../lib/data/preferences';
  import { writeHeaders } from '../../shared/write-token.js';
  import { onMount } from 'svelte';

  let { data, remotes, localDevice, lang, timezone, languageChanged, timezoneChanged, changed, settingsError = '' }: {
    settingsError?: string;
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
  const supportedTimezones = typeof Intl.supportedValuesOf === 'function' ? Intl.supportedValuesOf('timeZone') : ['Asia/Shanghai', 'Asia/Tokyo', 'America/New_York', 'America/Los_Angeles', 'Europe/London', 'Europe/Paris'];
  let homes = $state<string[]>([]), sourceError = $state('');
  let effectiveSources = $state<{ path: string; status: string }[]>([]);
  let sourceBusy = $state(false), sourcesReady = $state(false);
  const sourceLabels: Record<string, [string, string]> = { missing: ['未找到日志文件夹', 'Log folder not found'], unreadable: ['无法读取日志文件夹', 'Cannot read log folder'], empty: ['此文件夹尚无 Codex 日志', 'No Codex logs in this folder'] };
  async function loadSources() {
    const [settings, sources] = await Promise.all([
      preferences(),
      fetch('/api/sources').then(async r => { if (!r.ok) throw new Error('Cannot read log sources'); return r.json(); }),
    ]);
    homes = settings.codex_homes; effectiveSources = sources; sourcesReady = true;
  }
  onMount(() => { loadSources().catch(e => sourceError = String(e)); });
  async function applySources(paths: string[]) {
    if (isDesktop()) {
      await desktop('apply_sources', { codexHomes: paths });
      homes = paths;
      return;
    }
    const response = await fetch('/api/sources', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...await writeHeaders('') },
      body: JSON.stringify({ codex_homes: paths, expected_homes: effectiveSources.map(source => source.path) }),
    });
    const result = await response.json();
    if (!response.ok) {
      if (response.status === 409) await loadSources();
      throw new Error(result.error || 'Could not update log sources');
    }
    await loadSources();
    changed();
  }
  async function saveSources(paths: string[]) {
    sourceBusy = true; sourceError = '';
    try { await applySources(paths); return true; }
    catch (e) { sourceError = String(e); } finally { sourceBusy = false; }
    return false;
  }
  function sourcePathsWith(path: string): string[] | null {
    const paths = effectiveSources.map(source => source.path);
    if (paths.some(existing => sourcePathKey(existing) === sourcePathKey(path))) {
      sourceError = lang === 'zh' ? '此文件夹已添加' : 'This folder has already been added';
      return null;
    }
    return [...paths, path];
  }
  async function chooseSource() {
    if (sourceBusy || !sourcesReady) return;
    sourceBusy = true; sourceError = '';
    try {
      let path: string | null;
      if (isDesktop()) {
        path = await desktop<string | null>('choose_source');
      } else {
        const response = await fetch('/api/sources/choose', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...await writeHeaders('') },
          body: '{}',
        });
        const result = await response.json();
        if (!response.ok) throw new Error(lang === 'zh' ? '无法打开文件夹选择器：' + (result.error || '') : result.error || 'Could not open folder picker');
        path = result.path;
      }
      if (!path) return;
      const paths = sourcePathsWith(path);
      if (paths) await applySources(paths);
    } catch (e) { sourceError = String(e); } finally { sourceBusy = false; }
  }
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
      {#if settingsError}<p class="error" role="alert">{settingsError}</p>{/if}
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
    {#if isDesktop()}
      <div class="settings-card"><DesktopSettings {lang} /></div>
    {/if}
    {#if localDevice && localDevice.platform !== 'Darwin'}
      <section class="settings-card"><h2>{lang === 'zh' ? 'Codex 日志来源' : 'Codex log sources'}</h2>
        <ul class="source-list">
          {#each effectiveSources as source (source.path)}
            <li class="source-row">
              <div class="source-info"><div class="source-path">{displaySourcePath(source.path)}</div>{#if sourceLabels[source.status]}<div role="status">{sourceLabels[source.status][lang === 'zh' ? 0 : 1]}</div>{/if}</div>
              <button disabled={sourceBusy || !sourcesReady || effectiveSources.length <= 1} aria-label={`${lang === 'zh' ? '移除' : 'Remove'}: ${displaySourcePath(source.path)}`} onclick={() => saveSources(effectiveSources.filter(item => item.path !== source.path).map(item => item.path))}>{lang === 'zh' ? '移除' : 'Remove'}</button>
            </li>
          {/each}
        </ul>
        <div class="source-actions">
          <button disabled={sourceBusy || !sourcesReady || effectiveSources.length >= 32} onclick={chooseSource}>{lang === 'zh' ? '添加文件夹' : 'Add folder'}</button>
          {#if homes.length}<button disabled={sourceBusy || !sourcesReady} onclick={() => saveSources([])}>{lang === 'zh' ? '恢复自动发现' : 'Discover automatically'}</button>{/if}
        </div>
        {#if sourceError}<p class="error" role="alert">{sourceError}</p>{/if}
      </section>
    {/if}
    <div class="settings-card"><RemoteManager {data} {remotes} {localDevice} {lang} {changed} /></div>
  </div>
</div>

<style>
  .settings-header { margin-bottom: 24px; }
  h1 { margin: 0; }
  h2 { margin: 0 0 12px; }
  .source-list { display: grid; gap: 12px; padding: 0; margin: 0 0 16px; list-style: none; }
  .source-row { display: flex; align-items: center; gap: 12px; }
  .source-info { flex: 1; min-width: 0; }
  .source-path { overflow-wrap: anywhere; }
  .source-info [role='status'] { color: var(--muted); font-size: 12px; margin-top: 4px; }
  .source-row button { flex-shrink: 0; }
  .source-actions { display: flex; flex-wrap: wrap; gap: 10px; }
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
