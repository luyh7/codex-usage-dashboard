<script lang="ts">
  import { onMount } from 'svelte';
  import { dismissBackdrop } from '../lib/domain/dismiss';
  import { parseReleaseNotes } from '../lib/domain/releaseNotes';
  import type { Language } from '../lib/domain/ui';
  import Icon from './Icon.svelte';

  let { lang, currentVersion, version, notes, busy, error, downloaded, total, install, close }: {
    lang: Language;
    currentVersion: string;
    version?: string;
    notes?: string | null;
    busy: boolean;
    error: string;
    downloaded: number;
    total: number;
    install: () => void;
    close: () => void;
  } = $props();
  let panel: HTMLDialogElement;
  let release = $derived(parseReleaseNotes(notes, lang));
  let percent = $derived(total > 0 ? Math.min(100, Math.round(downloaded / total * 100)) : 0);
  const labels = { added: ['新增', 'Added'], fixed: ['修复', 'Fixed'], improved: ['优化', 'Improved'], feature: ['功能', 'Feature'] };
  const displayVersion = (value?: string) => value ? (value.startsWith('v') ? value : `v${value}`) : '—';
  function dismiss() { if (!busy) close(); }
  onMount(() => {
    panel.showModal();
    return () => { if (panel.open) panel.close(); };
  });
</script>

<dialog class="update-dialog" bind:this={panel} use:dismissBackdrop={dismiss} aria-labelledby="update-dialog-title" oncancel={(event) => { event.preventDefault(); dismiss(); }}>
  <header class="update-header">
    <div><span>{lang === 'zh' ? '版本更新' : 'Software update'}</span><h2 id="update-dialog-title">{lang === 'zh' ? '发现新版本' : 'A new version is available'}</h2></div>
    <button class="close" disabled={busy} aria-label={lang === 'zh' ? '关闭' : 'Close'} onclick={dismiss}><Icon name="close" size={18} /></button>
  </header>
  <div class="update-body">
    <div class="version-row">
      <div class="version-icon"><Icon name="arrow" size={20} /></div>
      <div class="version-copy"><span>{lang === 'zh' ? '可更新至' : 'Update to'}</span><strong>{displayVersion(version)}</strong></div>
      <div class="current-version">{lang === 'zh' ? '当前' : 'Current'} {displayVersion(currentVersion)}</div>
    </div>
    <div class="release-notes">
      <div class="notes-heading"><h3>{lang === 'zh' ? '版本内容' : 'Release notes'}</h3>{#if release.items.length}<span>{lang === 'zh' ? `${release.items.length} 项内容` : `${release.items.length} items`}</span>{/if}</div>
      {#if release.items.length}
        <ul>{#each release.items as item}<li><span class="note-type" class:fixed={item.kind === 'fixed'} class:neutral={item.kind === 'feature'}>{labels[item.kind][lang === 'zh' ? 0 : 1]}</span><span>{item.text}</span></li>{/each}</ul>
      {:else if release.fallback}
        <p class="raw-notes">{release.fallback}</p>
      {:else}
        <p class="empty-notes">{lang === 'zh' ? '暂无版本说明' : 'No release notes available'}</p>
      {/if}
    </div>
  </div>
  <div class="update-footer">
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if busy}
      <div class="update-progress"><div><span>{lang === 'zh' ? '正在更新' : 'Updating'}</span>{#if total > 0}<span>{percent}%</span>{/if}</div>{#if total > 0}<progress aria-label={lang === 'zh' ? '更新进度' : 'Update progress'} value={Math.min(downloaded, total)} max={total}></progress>{:else}<progress aria-label={lang === 'zh' ? '更新进度' : 'Update progress'}></progress>{/if}</div>
    {:else}
      <p class="restart-note">{lang === 'zh' ? '更新完成后将自动重启应用。' : 'The app will restart after the update.'}</p>
    {/if}
    <div class="update-actions"><button class="primary" disabled={busy} onclick={install}>{lang === 'zh' ? '更新' : 'Update'}</button><button disabled={busy} onclick={dismiss}>{lang === 'zh' ? '稍后再说' : 'Later'}</button></div>
  </div>
</dialog>

<style>
  .update-dialog { width: min(500px, calc(100vw - 32px)); max-height: calc(100dvh - 32px); padding: 0; overflow: hidden; border: 1px solid var(--border); border-radius: 20px; background: var(--panel); }
  .update-dialog[open] { display: flex; flex-direction: column; }
  .update-header { flex: 0 0 auto; padding: 25px 28px; background: linear-gradient(115deg, #ecfaf3, #f6fcf9); border-bottom: 1px solid #e4f1e9; }
  .update-header span { display: block; margin-bottom: 5px; color: #008a5d; font-size: 11px; font-weight: 700; letter-spacing: .08em; }
  .update-header h2 { margin: 0; font-size: 23px; line-height: 1.3; letter-spacing: -.4px; }
  .update-header button.close { align-self: flex-start; width: 34px; height: 34px; min-height: 34px; padding: 0; border: 1px solid transparent; border-radius: 8px; color: #70837f; background: transparent; }
  .update-header button.close:hover { border-color: #d6e9df; color: var(--accent-strong); background: #eff7f3; }
  .update-header button.close:active:not(:disabled) { background: #deefe6; }
  .update-body { flex: 1 1 auto; min-height: 0; overflow: auto; padding: 24px 28px 0; }
  .version-row { display: flex; align-items: center; gap: 12px; }
  .version-icon { display: grid; place-items: center; width: 43px; height: 43px; flex: 0 0 43px; color: #00875b; background: #e5f8ef; border-radius: 12px; }
  .version-icon :global(svg) { transform: rotate(180deg); }
  .version-copy { display: grid; gap: 1px; }
  .version-copy span { color: #798b90; font-size: 11px; }
  .version-copy strong { color: var(--heading); font-size: 17px; overflow-wrap: anywhere; }
  .current-version { margin-left: auto; padding: 5px 9px; border: 1px solid #e4ece9; border-radius: 7px; color: #788c8b; font-size: 11px; overflow-wrap: anywhere; }
  .release-notes { margin-top: 25px; }
  .notes-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 8px; }
  .notes-heading h3 { margin: 0; font-size: 14px; line-height: 1.4; letter-spacing: 0; }
  .notes-heading > span { color: #96a7a5; font-size: 11px; }
  .release-notes ul { margin: 0; padding: 0; list-style: none; }
  .release-notes li { display: flex; align-items: baseline; gap: 12px; padding: 12px 0; border-bottom: 1px solid #eef3f1; color: #526a72; font-size: 13px; overflow-wrap: anywhere; }
  .note-type { flex: 0 0 auto; padding: 2px 6px; border-radius: 5px; color: #00875b; background: #e8f8ef; font-size: 10px; font-weight: 600; }
  .note-type.fixed { color: #617991; background: #eef4f8; }
  .note-type.neutral { color: #6d7f82; background: #eef3f2; }
  .raw-notes { margin: 12px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; color: #526a72; font-size: 13px; }
  .empty-notes { margin: 12px 0 0; color: var(--muted); font-size: 13px; }
  .update-footer { flex: 0 0 auto; padding: 20px 28px 28px; }
  .update-footer p { margin: 0 0 14px; overflow-wrap: anywhere; font-size: 12px; }
  .update-footer .error { max-height: min(120px, 20dvh); overflow: auto; }
  .restart-note { color: #70847f; }
  .update-progress { margin-bottom: 14px; color: var(--muted); font-size: 12px; }
  .update-progress > div { display: flex; justify-content: space-between; gap: 12px; }
  .update-progress progress { display: block; width: 100%; height: 8px; margin-top: 8px; accent-color: var(--accent); }
  .update-actions { display: grid; grid-template-columns: 1fr 108px; gap: 10px; }
  .update-actions button { height: 42px; min-height: 42px; font-size: 13px; }
  .update-actions .primary:hover:not(:disabled) { background: #007f54; }
  .update-actions .primary:active:not(:disabled) { background: #006d49; }
  @media (max-width: 600px) {
    .update-header { padding: 22px 24px; }
    .update-body { padding: 22px 24px 0; }
    .update-footer { padding: 18px 24px 24px; }
  }
</style>
