<script lang="ts">
  import type { DashboardData, Remote, LocalDevice, ImportResult } from '../lib/data/types';
  import { words, number, type Language } from '../lib/domain/ui';
  import { downloadSnapshot } from '../../shared/snapshot-export.js';
  let {
    data,
    remotes,
    localDevice,
    lang,
    changed,
  }: {
    data: DashboardData;
    remotes: Remote[];
    localDevice?: LocalDevice;
    lang: Language;
    changed: () => void;
  } = $props();
  let exporting = $state(false), exportPercent = $state(0), exportPhase = $state('');
  let exportMode = $state<'full' | 'redacted'>('full');
  let fileInput: HTMLInputElement;
  const exportLabels = {
    zh: { preparing: '准备快照', compressing: '正在压缩', downloading: '正在下载', complete: '已导出' },
    en: { preparing: 'Preparing snapshot', compressing: 'Compressing', downloading: 'Downloading', complete: 'Exported' },
  };
  async function exportingSnapshot(mode: 'full' | 'redacted' = 'full') {
    if (exporting) return;
    exporting = true; exportMode = mode; error = ''; exportPercent = 0; exportPhase = 'preparing';
    try {
      await downloadSnapshot(data.exportUrl(mode), (percent, phase) => { exportPercent = percent; exportPhase = phase; });
    } catch (e) { error = String(e); exportPhase = ''; }
    finally { exporting = false; }
  }
  let t = $derived(words(lang));
  let error = $state(''),
    busy = $state(false),
    label = $state('');
  let incoming = $state.raw<unknown>(null),
    result = $state.raw<ImportResult | null>(null);
  let deleting = $state(''),
    renaming = $state(''),
    renameLabel = $state('');
  async function read(event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (!file) return;
    fileInput.value = '';
    try {
      if (file.size > 512 * 1024 * 1024) throw new Error('Snapshot exceeds 512 MiB');
      incoming = file;
      await importing();
    } catch (e) {
      error = String(e);
    }
  }
  async function importing(allow = false) {
    busy = true;
    error = '';
    try {
      result = await data.importSnapshot(incoming, label || undefined, allow);
      if (result.needs_label || result.needs_confirmation) label = result.suggested_label || label;
      if (result.ok) {
        incoming = null;
        label = '';
        changed();
      }
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  async function remove(code: string) {
    busy = true;
    error = '';
    try {
      await data.deleteRemote(code);
      deleting = '';
      changed();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  async function rename(code: string) {
    busy = true;
    error = '';
    try {
      await data.renameRemote(code, renameLabel);
      renaming = '';
      changed();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="device-settings" aria-labelledby="remote-title">
  <h2 id="remote-title">{t.devices}</h2>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="remote-actions">
    <button class="button" disabled={busy} onclick={() => fileInput.click()}>{t.import}</button>
    <input
        bind:this={fileInput!}
        class="snapshot-input"
        aria-label={t.import}
        type="file"
        accept=".json,.json.gz,.gz,application/json,application/gzip,application/x-gzip"
        onchange={read}
        disabled={busy}
      />
  </div>
  {#if result?.needs_label || result?.needs_confirmation}<section class="notice">
      {#if result.needs_confirmation}<p>{t.currentConfirm}</p>{/if}
      <label>{t.label}<input bind:value={label} /></label><button
        class="primary"
        disabled={busy || !label.trim()}
        onclick={() => importing(!!result?.needs_confirmation)}>{t.confirm}</button
      ><button
        onclick={() => {
          incoming = null;
          result = null;
        }}>{t.cancel}</button
      >
    </section>{/if}
  <article class="remote-row local-device-row">
    <div>
      <div class="device-name"><b>{localDevice?.label || t.thisDevice}</b>{#if localDevice?.label}<span class="device-kind local-kind">{t.thisDevice}</span>{/if}</div>
      {#if localDevice}<small>{localDevice.device_short_code} · {number(localDevice.session_count, lang)} {t.sessions}</small>{/if}
      {#if localDevice?.sources && localDevice.sources.length > 1}
        <ul class="local-sources">
          {#each localDevice.sources as source (source.id)}
            <li><span>{source.label}</span><span>{number(source.session_count, lang)} {t.sessions}</span></li>
          {/each}
        </ul>
      {/if}
    </div>
    <div class="export-options">
      <div class="remote-buttons">
        {#each ['full', 'redacted'] as mode}
          <div class="export-option">
          <button class="button" disabled={exporting} onclick={() => exportingSnapshot(mode as 'full' | 'redacted')} aria-busy={exporting && exportMode === mode}>
            {#if exporting && exportMode === mode}<span class="export-spinner" aria-hidden="true"></span>{/if}
            {mode === 'full' ? (lang === 'zh' ? '完整导出' : 'Full export') : (lang === 'zh' ? '脱敏导出' : 'Redacted export')}
          </button>
          {#if mode === 'full'}<small>{lang === 'zh' ? '包含会话摘要和本地路径' : 'Includes session summaries and local paths'}</small>{/if}
          </div>
        {/each}
      </div>
    </div>
    {#if exportPhase}<div class="export-progress" aria-live="polite">
      <div class="export-caption"><span>{exportLabels[lang][exportPhase as keyof typeof exportLabels.zh]}</span><strong>{exportPercent}%</strong></div>
      <div class="export-track" role="progressbar" aria-label={t.export} aria-valuemin="0" aria-valuemax="100" aria-valuenow={exportPercent}><span style:width={`${exportPercent}%`}></span></div>
    </div>{/if}
  </article>
  {#if !remotes.length}<p class="muted">
      {t.remoteEmpty}
    </p>{/if}{#each remotes as remote (remote.device_short_code)}<article class="remote-row imported-device-row">
      <div>
        <div class="device-name"><b>{remote.label}</b><span class="device-kind">{t.importedDevice}</span></div><small
          >{remote.device_short_code} · {number(remote.session_count, lang)} {t.sessions}</small
        >
      </div>
      <div class="remote-buttons">
        <button
          disabled={busy}
          onclick={() => {
            renaming = remote.device_short_code;
            renameLabel = remote.label;
          }}>{t.rename}</button
        ><button disabled={busy} onclick={() => (deleting = remote.device_short_code)}
          >{t.delete}</button
        >
      </div>
      {#if renaming === remote.device_short_code}<form
          onsubmit={(e) => {
            e.preventDefault();
            rename(remote.device_short_code);
          }}
        >
          <input aria-label={t.label} bind:value={renameLabel} /><button disabled={busy}
            >{t.confirm}</button
          ><button type="button" onclick={() => (renaming = '')}>{t.cancel}</button>
        </form>{/if}{#if deleting === remote.device_short_code}<div class="notice">
          <p>{t.deleteConfirm}</p>
          <button disabled={busy} onclick={() => remove(remote.device_short_code)}
            >{t.confirm}</button
          ><button onclick={() => (deleting = '')}>{t.cancel}</button>
        </div>{/if}
    </article>{/each}
</section>
<style>
  h2 { margin: 0 0 20px; }
  .snapshot-input { display: none; }
  .remote-actions { margin-bottom: 22px; flex-wrap: wrap; }
  .remote-row:last-child { border-bottom: 0; padding-bottom: 0; }
  .remote-row > div:first-child { min-width: 0; overflow-wrap: anywhere; }
  .device-settings > .notice { display: flex; flex-wrap: wrap; align-items: end; gap: 10px; }
  .device-settings > .notice > p { flex-basis: 100%; margin-bottom: 0; }
  .device-settings > .notice > label { display: grid; gap: 8px; flex: 1 1 180px; min-width: 0; }
  .device-settings > .notice input { width: 100%; }
  .remote-row small { overflow-wrap: anywhere; }
  .device-name { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .device-kind { padding: 2px 7px; border-radius: 5px; background: var(--bg); color: var(--muted); font-size: 11px; line-height: 18px; font-weight: 500; }
  .local-kind { background: #34b89b18; color: var(--accent); }
  .local-sources { display: grid; gap: 6px; margin: 12px 0 0; padding: 0 0 0 12px; border-left: 2px solid var(--border); list-style: none; font-size: 12px; line-height: 18px; }
  .local-sources li { display: flex; flex-wrap: wrap; align-items: baseline; gap: 6px 16px; }
  .local-sources li > span:last-child { color: var(--muted); font-variant-numeric: tabular-nums; }
  .export-options { align-self: flex-start; display: grid; gap: 8px; }
  .export-options .remote-buttons { flex-wrap: wrap; }
  .export-option { display: grid; gap: 8px; align-content: start; }
  .export-option small { max-width: 180px; }
  .remote-row form, .remote-row .notice { min-width: 0; }
  .remote-row form input { min-width: 0; }
  @media (max-width: 600px) {
    .remote-row { flex-wrap: wrap; gap: 12px; }
    .remote-row > div:first-child { flex: 1 1 100%; }
    .remote-row form { flex-wrap: wrap; }
  }
  .export-progress { flex-basis: 100%; margin: 2px 0 0; }
  .export-caption { display: flex; justify-content: space-between; margin-bottom: 8px; font-size: 13px; }
  .export-caption strong { font-variant-numeric: tabular-nums; }
  .export-track { height: 6px; overflow: hidden; border-radius: 6px; background: #8899aa24; }
  .export-track span { display: block; height: 100%; background: #34b89b; border-radius: inherit; transition: width 180ms ease-out; }
  .export-spinner { display: inline-block; width: 13px; height: 13px; border: 2px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: export-spin .8s linear infinite; }
  @keyframes export-spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .export-spinner { animation: none; } .export-track span { transition: none; } }
</style>
