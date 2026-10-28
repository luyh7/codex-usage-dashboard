<script lang="ts">
  import type { SnapshotImportState } from '../lib/data/types';
  import { words, type Language } from '../lib/domain/ui';
  import ElapsedTime from './ElapsedTime.svelte';
  let { state, lang }: { state: SnapshotImportState; lang: Language } = $props();
  let t = $derived(words(lang));
</script>

<section class="import-status" role="status">
  <div class="top">
    <div class="mark" aria-hidden="true">{#if state.phase === 'working'}<span class="spinner"></span>{:else}<span class="check">✓</span>{/if}</div>
    <div class="copy"><strong>{state.phase === 'working' ? t.importWorking : t.importComplete}</strong><p>{state.phase === 'working' ? t.importWorkingHint : t.importCompleteHint}</p></div>
    {#if state.phase === 'working'}<ElapsedTime {lang} startedAt={state.startedAt} />{/if}
  </div>
  {#if state.phase === 'working'}
    <div class="track" role="progressbar" aria-label={t.importWorking}><span></span></div>
    <div class="hint">{t.importUpdateHint}</div>
  {/if}
</section>

<style>
  .import-status { margin: 0 0 18px; padding: 16px; background: #f2fbf6; border: 1px solid #cdebdc; border-radius: 12px; }
  .top { display: flex; align-items: flex-start; gap: 12px; }
  .mark { display: grid; place-items: center; flex: 0 0 auto; width: 38px; height: 38px; border-radius: 12px; background: #dff6ea; color: #008655; }
  .spinner { width: 19px; height: 19px; border: 2px solid #91d4b9; border-top-color: #008655; border-radius: 50%; animation: spin .9s linear infinite; }.check { font-size: 19px; font-weight: 800; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .copy { flex: 1; min-width: 0; }.copy strong { color: #125e46; font-size: 13px; }.copy p { margin: 4px 0 0; color: #6a7f87; font-size: 12px; line-height: 1.7; }
  .track { height: 4px; overflow: hidden; background: #d8eee2; border-radius: 4px; margin-top: 15px; }.track span { display: block; height: 100%; width: 38%; border-radius: inherit; background: #1ca979; animation: travel 1.6s ease-in-out infinite; }
  @keyframes travel { from { transform: translateX(-110%); } to { transform: translateX(270%); } }
  .hint { margin-top: 7px; color: #718e82; font-size: 11px; }
  @media (prefers-reduced-motion: reduce) { .spinner, .track span { animation: none; } }
</style>
