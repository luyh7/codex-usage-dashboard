<script lang="ts">
  import { onMount } from 'svelte';
  import { desktop } from '../lib/data/desktop';
  import type { Language } from '../lib/domain/ui';
  import Dropdown from './Dropdown.svelte';
  let { lang }: { lang: Language } = $props();
  type Settings = { hide_on_close: boolean; can_hide: boolean; autostart: boolean; version: string; channel: string };
  let value = $state<Settings | null>(null), busy = $state(false), error = $state('');
  let update = $state<{ state: string; version?: string; notes?: string; message?: string } | null>(null);
  let downloaded = $state(0), total = $state(0);
  onMount(() => {
    desktop<Settings>('desktop_settings').then(v => value = v).catch(e => error = String(e));
    const progress = (event: Event) => { const detail = (event as CustomEvent).detail; downloaded += detail.received; total = detail.total || 0; };
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
    busy = true; error = ''; downloaded = 0; total = 0;
    try { update = await desktop('check_update', { install }); } catch (e) { error = String(e); } finally { busy = false; }
  }
</script>
<section aria-labelledby="desktop-title">
  <h2 id="desktop-title">{lang === 'zh' ? '客户端' : 'Desktop'}</h2>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if value}
    <div class="row"><span>{lang === 'zh' ? '关闭窗口' : 'Close window'}</span><Dropdown label={lang === 'zh' ? '关闭窗口' : 'Close window'} value={value.hide_on_close ? 'hide' : 'quit'} options={value.can_hide ? [['quit', lang === 'zh' ? '退出应用' : 'Quit'], ['hide', lang === 'zh' ? '保留在托盘' : 'Keep in tray']] : [['quit', lang === 'zh' ? '系统默认' : 'System default']]} change={v => save({ hide_on_close: v === 'hide' })} /></div>
    <div class="row"><label for="autostart">{lang === 'zh' ? '登录时启动' : 'Start at login'}</label><input id="autostart" type="checkbox" checked={value.autostart} disabled={busy} onchange={e => save({ autostart: e.currentTarget.checked })} /></div>
    <div class="row"><span>{value.version}</span><button disabled={busy} onclick={() => check()}>{lang === 'zh' ? '检查更新' : 'Check for updates'}</button></div>
    {#if update?.state === 'current'}<p role="status">{lang === 'zh' ? '已是最新版本' : 'Up to date'}</p>{/if}
    {#if update?.state === 'unavailable'}<p role="status">{lang === 'zh' ? '此开发包尚未配置签名更新渠道。' : update.message}</p>{/if}
    {#if update?.state === 'package'}<p role="status">{lang === 'zh' ? '请通过包管理器或新版安装包更新。' : update.message}</p>{/if}
    {#if update?.state === 'available'}<p>{update.version}</p><p class="notes">{update.notes}</p><button class="primary" disabled={busy} onclick={() => check(true)}>{lang === 'zh' ? '安装并重启' : 'Install and restart'}</button>{/if}
    {#if busy && total > 0}<progress value={downloaded} max={total}></progress>{/if}
  {/if}
</section>
<style>h2{margin:0 0 12px}.row{display:flex;align-items:center;justify-content:space-between;gap:20px;padding:20px 0}.row>span,.row>label{flex-shrink:0}.row+.row{border-top:1px solid var(--border)}.row :global(.dropdown){min-width:140px;flex:1}.notes{white-space:pre-wrap}progress{width:100%}</style>
