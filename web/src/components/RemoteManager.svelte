<script lang="ts">
  import { onMount } from 'svelte';
  import type { DashboardData, Remote, ImportResult } from '../lib/data/types';
  import { words, number, type Language } from '../lib/domain/ui';
  let {
    data,
    remotes,
    lang,
    close,
    changed,
  }: {
    data: DashboardData;
    remotes: Remote[];
    lang: Language;
    close: () => void;
    changed: () => void;
  } = $props();
  let dialog: HTMLDialogElement;
  let t = $derived(words(lang));
  let error = $state(''),
    busy = $state(false),
    label = $state('');
  let incoming = $state.raw<unknown>(null),
    result = $state.raw<ImportResult | null>(null);
  let deleting = $state(''),
    renaming = $state(''),
    renameLabel = $state('');
  onMount(() => {
    dialog.showModal();
    return () => dialog.close();
  });
  async function read(event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (!file) return;
    try {
      if (file.size > 100 * 1024 * 1024) throw new Error('JSON exceeds 100 MB');
      incoming = JSON.parse((await file.text()).replace(/^\uFEFF/, ''));
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

<dialog bind:this={dialog!} onclose={close} oncancel={close} aria-labelledby="remote-title">
  <header>
    <h2 id="remote-title">{t.remote}</h2>
    <button aria-label={t.close} onclick={close}>×</button>
  </header>
  <p class="muted">{t.subtitle}</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="remote-actions">
    <label class="file-button"
      >{t.import}<input
        type="file"
        accept=".json,application/json"
        onchange={read}
        disabled={busy}
      /></label
    ><a class="button" href={data.exportUrl()}>{t.export}</a>
  </div>
  {#if result?.needs_label || result?.needs_confirmation}<section class="notice">
      <p>{result.needs_confirmation ? t.currentConfirm : t.label}</p>
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
  {#if !remotes.length}<p class="muted">
      {t.remoteEmpty}
    </p>{/if}{#each remotes as remote (remote.device_short_code)}<article class="remote-row">
      <div>
        <b>{remote.label}</b><small
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
</dialog>
