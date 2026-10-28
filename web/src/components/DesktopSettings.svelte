<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { desktop } from '../lib/data/desktop';
  import type { Language } from '../lib/domain/ui';
  import Dropdown from './Dropdown.svelte';
  import UpdateDialog from './UpdateDialog.svelte';

  let { lang }: { lang: Language } = $props();
  type Settings = { hide_on_close: boolean; can_hide: boolean; autostart: boolean; version: string; channel: string };
  type Update = { state: string; version?: string; notes?: string | null; message?: string };
  let value = $state<Settings | null>(null), busy = $state(false), error = $state(''), updateError = $state('');
  let update = $state<Update | null>(null);
  let downloaded = $state(0), total = $state(0);
  let checkTrigger = $state<HTMLButtonElement>();

  onMount(() => {
    desktop<Settings>('desktop_settings').then(v => value = v).catch(e => error = String(e));
    const progress = (event: Event) => {
      const detail = (event as CustomEvent).detail;
      if (typeof detail?.received === 'number' && Number.isFinite(detail.received) && detail.received > 0) downloaded += detail.received;
      if (typeof detail?.total === 'number' && Number.isFinite(detail.total) && detail.total > 0) total = detail.total;
    };
    window.addEventListener('cousash-update-progress', progress);
    return () => window.removeEventListener('cousash-update-progress', progress);
  });

  async function save(patch: Partial<Settings>) {
    if (!value || busy) return;
    busy = true; error = '';
    try { const next = { ...value, ...patch }; await desktop('save_desktop_settings', { hideOnClose: next.hide_on_close, autostart: next.autostart }); value = next; }
    catch (e) { error = String(e); } finally { busy = false; }
  }

  async function check(install = false) {
    if (busy) return;
    busy = true; error = ''; updateError = ''; downloaded = 0; total = 0;
    if (!install) update = null;
    try { update = await desktop<Update>('check_update', { install }); }
    catch (e) { if (install) updateError = String(e); else error = String(e); }
    finally { busy = false; }
  }

  async function dismissUpdate() {
    if (busy) return;
    update = null; updateError = '';
    await tick();
    checkTrigger?.focus({ preventScroll: true });
  }
</script>

<section aria-labelledby="desktop-title">
  <h2 id="desktop-title">{lang === 'zh' ? '客户端' : 'Desktop'}</h2>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if value}
    <div class="row"><span>{lang === 'zh' ? '关闭窗口' : 'Close window'}</span><Dropdown label={lang === 'zh' ? '关闭窗口' : 'Close window'} value={value.hide_on_close ? 'hide' : 'quit'} options={value.can_hide ? [['quit', lang === 'zh' ? '退出应用' : 'Quit'], ['hide', lang === 'zh' ? '保留在托盘' : 'Keep in tray']] : [['quit', lang === 'zh' ? '系统默认' : 'System default']]} change={v => save({ hide_on_close: v === 'hide' })} /></div>
    <div class="row"><label for="autostart">{lang === 'zh' ? '登录时启动' : 'Start at login'}</label><input id="autostart" type="checkbox" checked={value.autostart} disabled={busy} onchange={e => { const autostart = e.currentTarget.checked; e.currentTarget.checked = value?.autostart ?? false; void save({ autostart }); }} /></div>
    <div class="row"><span>{value.version}</span><button bind:this={checkTrigger} disabled={busy} onclick={() => check()}>{lang === 'zh' ? '检查更新' : 'Check for updates'}</button></div>
    {#if update?.state === 'current'}<p role="status">{lang === 'zh' ? '已是最新版本' : 'Up to date'}</p>{/if}
    {#if update?.state === 'unavailable'}<p role="status">{lang === 'zh' ? '尚未配置自动更新，请使用新版完整安装包升级。' : update.message}</p>{/if}
    {#if update?.state === 'package'}<p role="status">{lang === 'zh' ? '请通过包管理器或新版安装包更新。' : update.message}</p>{/if}
    {#if update?.state === 'available'}
      <UpdateDialog {lang} currentVersion={value.version} version={update.version} notes={update.notes} {busy} error={updateError} {downloaded} {total} install={() => check(true)} close={dismissUpdate} />
    {/if}
  {/if}
</section>

<style>
  h2 { margin: 0 0 12px; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 20px; padding: 20px 0; }
  .row > span, .row > label { flex-shrink: 0; }
  .row + .row { border-top: 1px solid var(--border); }
  .row :global(.dropdown) { min-width: 140px; flex: 1; }
</style>
