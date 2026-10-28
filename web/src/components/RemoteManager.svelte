<script lang="ts">
  import type { DashboardData, Remote, LocalDevice, ImportResult, SnapshotImportState } from '../lib/data/types';
  import { words, number, type Language } from '../lib/domain/ui';
  import { downloadSnapshot } from '../../shared/snapshot-export.js';
  import Icon from './Icon.svelte';
  import SnapshotImportStatus from './SnapshotImportStatus.svelte';
  import { isDesktop, desktop } from '../lib/data/desktop';
  let {
    data,
    remotes,
    localDevice,
    lang,
    changed,
    snapshotImport,
    snapshotImportChanged,
  }: {
    snapshotImport: SnapshotImportState;
    snapshotImportChanged: (value: SnapshotImportState) => void;
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
      if (isDesktop()) {
        const progress = (event: Event) => { const detail = (event as CustomEvent).detail; exportPercent = detail.percent; exportPhase = detail.phase; };
        window.addEventListener('cousash-export-progress', progress);
        try { const saved = await desktop<boolean>('export_snapshot', { redacted: mode === 'redacted' }); exportPhase = saved ? 'complete' : ''; if (saved) exportPercent = 100; }
        finally { window.removeEventListener('cousash-export-progress', progress); }
      } else await downloadSnapshot(data.exportUrl(mode), (percent, phase) => { exportPercent = percent; exportPhase = phase; });
    } catch (e) { error = String(e); exportPhase = ''; }
    finally { exporting = false; }
  }
  let t = $derived(words(lang));
  let error = $state(''),
    busy = $state(false);
  let deleting = $state(''),
    renaming = $state(''),
    renameLabel = $state('');
  let controlsBusy = $derived(busy || snapshotImport.phase === 'working');
  async function read(event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (!file || controlsBusy) return;
    fileInput.value = '';
    error = '';
    if (file.size > 512 * 1024 * 1024) {
      snapshotImportChanged({ phase: 'error', error: 'Snapshot exceeds 512 MiB' });
      return;
    }
    await importing(file);
  }
  async function importing(incoming: unknown, label = '', confirmation?: ImportResult) {
    if (controlsBusy) return;
    busy = true;
    error = '';
    const attempt = { incoming, label, confirmation };
    snapshotImportChanged({ ...attempt, phase: 'working', startedAt: Date.now() });
    try {
      const result = await data.importSnapshot(incoming, label || undefined, !!confirmation?.needs_confirmation);
      if (result.needs_label || result.needs_confirmation) {
        snapshotImportChanged({ phase: 'confirmation', incoming, label: result.suggested_label || label, confirmation: result });
      } else if (result.ok) {
        snapshotImportChanged({ phase: 'success' });
        changed();
      } else {
        snapshotImportChanged({ phase: 'idle' });
      }
    } catch (e) {
      snapshotImportChanged({ ...attempt, phase: 'error', error: String(e) });
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
  <header class="device-header">
    <h2 id="remote-title">{t.devices}</h2>
    <button class="primary" disabled={controlsBusy} onclick={() => fileInput.click()}><Icon name="import" size={16} />{t.import}</button>
  </header>
  {#if snapshotImport.phase === 'working' || snapshotImport.phase === 'success'}<SnapshotImportStatus state={snapshotImport} {lang} />{/if}
  {#if snapshotImport.phase === 'error'}<p class="error" role="alert">{snapshotImport.error}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <input
        bind:this={fileInput!}
        class="snapshot-input"
        aria-label={t.import}
        type="file"
        accept=".json,.json.gz,.gz,application/json,application/gzip,application/x-gzip"
        onchange={read}
        disabled={controlsBusy}
    />
  {#if !controlsBusy && snapshotImport.confirmation}<section class="notice">
      {#if snapshotImport.confirmation.needs_confirmation}<p>{t.currentConfirm}</p>{/if}
      <label>{t.label}<input value={snapshotImport.label || ''} oninput={(event) => snapshotImportChanged({ ...snapshotImport, label: event.currentTarget.value })} /></label><button
        class="primary"
        disabled={!snapshotImport.label?.trim()}
        onclick={() => importing(snapshotImport.incoming, snapshotImport.label, snapshotImport.confirmation)}>{t.confirm}</button
      ><button onclick={() => snapshotImportChanged({ phase: 'idle' })}>{t.cancel}</button>
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
          <button class="button" disabled={exporting} onclick={() => exportingSnapshot(mode as 'full' | 'redacted')} aria-busy={exporting && exportMode === mode}
            data-tooltip={mode === 'full'
              ? (lang === 'zh' ? '包含会话摘要和本地路径。' : 'Includes session summaries and local paths.')
              : (lang === 'zh' ? '移除会话摘要和本地路径，匿名化设备与会话标识，保留用量和费用。' : 'Removes session summaries and local paths, anonymizes device and session IDs, and keeps usage and costs.')}>
            {#if exporting && exportMode === mode}<span class="export-spinner" aria-hidden="true"></span>{:else}<Icon name={mode === 'full' ? 'export' : 'shield'} size={16} />{/if}
            {mode === 'full' ? (lang === 'zh' ? '完整导出' : 'Full export') : (lang === 'zh' ? '脱敏导出' : 'Redacted export')}
          </button>
        {/each}
      </div>
    </div>
    {#if exportPhase}<div class="export-progress" aria-live="polite">
      <div class="export-caption"><span>{exportLabels[lang][exportPhase as keyof typeof exportLabels.zh]}</span><strong>{exportPercent}%</strong></div>
      <div class="export-track" role="progressbar" aria-label={t.export} aria-valuemin="0" aria-valuemax="100" aria-valuenow={exportPercent}><span style:width={`${exportPercent}%`}></span></div>
    </div>{/if}
  </article>
  {#if !remotes.length}<p class="muted remote-empty">
      {t.remoteEmpty}
    </p>{/if}{#each remotes as remote (remote.device_short_code)}<article class="remote-row imported-device-row">
      <div>
        <div class="device-name"><b>{remote.label}</b><span class="device-kind">{t.importedDevice}</span></div><small
          >{remote.device_short_code} · {number(remote.session_count, lang)} {t.sessions}</small
        >
      </div>
      <div class="remote-buttons">
        <button
          disabled={controlsBusy}
          onclick={() => {
            renaming = remote.device_short_code;
            renameLabel = remote.label;
          }}>{t.rename}</button
        ><button disabled={controlsBusy} onclick={() => (deleting = remote.device_short_code)}
          >{t.delete}</button
        >
      </div>
      {#if renaming === remote.device_short_code}<form
          onsubmit={(e) => {
            e.preventDefault();
            rename(remote.device_short_code);
          }}
        >
          <input aria-label={t.label} bind:value={renameLabel} /><button disabled={controlsBusy}
            >{t.confirm}</button
          ><button type="button" onclick={() => (renaming = '')}>{t.cancel}</button>
        </form>{/if}{#if deleting === remote.device_short_code}<div class="notice">
          <p>{t.deleteConfirm}</p>
          <button disabled={controlsBusy} onclick={() => remove(remote.device_short_code)}
            >{t.confirm}</button
          ><button onclick={() => (deleting = '')}>{t.cancel}</button>
        </div>{/if}
    </article>{/each}
</section>
<style>
  .device-settings { container-type: inline-size; }
  .device-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin-bottom: 22px; }
  .local-device-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: start; gap: 16px 24px; }
  .device-settings :global(button) {
    border-color: #a8cbbd;
    background: var(--accent-bg);
    color: var(--accent-strong);
    font-weight: 600;
    box-shadow: 0 1px 2px #103d2d12;
  }
  .device-settings :global(button:hover:not(:disabled)) {
    border-color: #72ae96;
    background: #d8f3e6;
    box-shadow: 0 2px 4px #103d2d18;
  }
  .device-settings :global(button.primary) {
    border-color: #007348;
    background: #008653;
    color: #fff;
  }
  .device-settings :global(button.primary:hover:not(:disabled)) { background: #006e44; }
  .device-settings :global(button:active:not(:disabled)) { box-shadow: inset 0 1px 2px #103d2d24; }
  h2 { margin: 0; }
  .snapshot-input { display: none; }
  .remote-row:last-child { border-bottom: 0; padding-bottom: 0; }
  .remote-empty { margin: 0; padding-top: 17px; border-top: 1px solid var(--border); }
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
  .export-options { display: flex; justify-content: flex-end; }
  .remote-buttons { gap: 10px; }
  .export-options .remote-buttons { align-items: start; }
  .remote-row form, .remote-row .notice { min-width: 0; }
  .remote-row form input { min-width: 0; }
  @container (max-width: 460px) {
    .local-device-row { grid-template-columns: minmax(0, 1fr); }
    .export-options { justify-content: flex-start; }
    .export-options .remote-buttons { flex-wrap: wrap; }
  }
  @media (max-width: 600px) {
    .remote-row { flex-wrap: wrap; gap: 12px; }
    .remote-row > div:first-child { flex: 1 1 100%; }
    .remote-row form { flex-wrap: wrap; }
  }
  .export-progress { grid-column: 1 / -1; margin: 2px 0 0; }
  .export-caption { display: flex; justify-content: space-between; margin-bottom: 8px; font-size: 13px; }
  .export-caption strong { font-variant-numeric: tabular-nums; }
  .export-track { height: 6px; overflow: hidden; border-radius: 6px; background: #8899aa24; }
  .export-track span { display: block; height: 100%; background: #34b89b; border-radius: inherit; transition: width 180ms ease-out; }
  .export-spinner { display: inline-block; width: 13px; height: 13px; border: 2px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: export-spin .8s linear infinite; }
  @keyframes export-spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .export-spinner { animation: none; } .export-track span { transition: none; } }
</style>
