<script lang="ts">
  import type { IndexStatus } from '../lib/data/types';
  import { number, words, type Language } from '../lib/domain/ui';
  import ElapsedTime from './ElapsedTime.svelte';

  let { status, lang }: { status: IndexStatus; lang: Language } = $props();
  let t = $derived(words(lang));
  let phase = $derived.by(() => {
    if (status.phase === 'parsing') return t.indexReading;
    if (status.phase === 'normalizing') return t.indexOrganizing;
    if (status.phase === 'committing') return t.indexSaving;
    if (status.phase === 'ready') return t.indexLoading;
    return t.indexPreparing;
  });
  // Only parsing reports a meaningful count of files processed; later phases
  // report different totals and do not expose their own incremental progress.
  let files = $derived.by(() => {
    if (status.phase !== 'parsing' || typeof status.completed !== 'number' ||
        typeof status.total !== 'number' || !Number.isFinite(status.completed) ||
        !Number.isFinite(status.total) || status.completed < 0 || status.total <= 0) return null;
    return { completed: Math.min(status.completed, status.total), total: status.total };
  });
</script>

<section class="index-waiting" aria-label={status.phase === 'ready' ? t.indexLoading : t.indexing}>
  <div class="index-card">
    <div class="mark" aria-hidden="true"><span class="spinner"></span></div>
    <div class="copy">
      <div class="eyebrow">{t.indexPreparingData}</div>
      <h2>{status.phase === 'ready' ? t.indexLoading : t.indexing}</h2>
      <p>{status.phase === 'ready' ? t.indexLoadingHint : t.indexHint}</p>
      <div class="progress-heading"><span role="status">{phase}</span>{#if files}<strong>{number(files.completed, lang)} / {number(files.total, lang)}</strong>{/if}</div>
      {#if files}
        <div class="track" role="progressbar" aria-label={t.indexReading} aria-valuemin="0" aria-valuemax={files.total} aria-valuenow={files.completed}><span style:width={`${files.completed / files.total * 100}%`}></span></div>
      {:else}
        <div class="track" role="progressbar" aria-label={phase}><span class="indeterminate"></span></div>
      {/if}
      <div class="progress-foot">{#if files}<span>{t.indexReadingHint}</span>{/if}<ElapsedTime {lang} /></div>
    </div>
  </div>
  <div class="placeholder-metrics" aria-hidden="true">{#each [0, 1, 2, 3] as item}<div><i></i><b></b><span></span></div>{/each}</div>
  <div class="placeholder-list" aria-hidden="true"><div class="placeholder-title"><b></b><i></i></div>{#each [0, 1, 2, 3, 4] as item}<div class="placeholder-row"><i></i><span><b></b><small></small></span><em></em></div>{/each}</div>
</section>

<style>
  .index-waiting { flex: 1 1 auto; min-height: 0; overflow: auto; padding: 2px 2px 28px; }
  .index-card { display: flex; gap: 18px; padding: 23px 26px; border: 1px solid #cae8db; border-radius: 16px; background: linear-gradient(110deg, #f0fbf6, #fff 75%); }
  .mark { display: grid; place-items: center; flex: 0 0 auto; width: 46px; height: 46px; border-radius: 14px; background: #dff6ea; }
  .spinner { width: 19px; height: 19px; border: 2px solid #91d4b9; border-top-color: #008655; border-radius: 50%; animation: spin .9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .copy { width: 100%; min-width: 0; }
  .eyebrow { color: #038561; font-size: 11px; font-weight: 700; letter-spacing: .08em; margin-bottom: 7px; }
  h2 { margin: 0; color: #102e2a; font-size: 19px; letter-spacing: -.3px; }
  p { margin: 3px 0 0; color: #6a7f87; font-size: 13px; line-height: 1.7; }
  .progress-heading { display: flex; justify-content: space-between; gap: 15px; margin-top: 20px; color: #58736a; font-size: 12px; }
  .progress-heading strong { color: #17694f; font-variant-numeric: tabular-nums; }
  .track { overflow: hidden; height: 7px; border-radius: 10px; background: #dceee6; margin-top: 8px; }
  .track span { display: block; height: 100%; border-radius: inherit; background: #18a776; transition: width 180ms ease-out; }
  .track .indeterminate { width: 35%; animation: travel 1.6s ease-in-out infinite; }
  @keyframes travel { from { transform: translateX(-110%); } to { transform: translateX(290%); } }
  .progress-foot { display: flex; justify-content: space-between; gap: 8px; margin-top: 8px; color: #718e82; font-size: 11px; }
  .progress-foot :global(.elapsed) { margin-left: auto; }
  .placeholder-metrics { display: grid; grid-template-columns: 1.4fr 1fr 1fr 1fr; gap: 14px; margin-top: 16px; }
  .placeholder-metrics div, .placeholder-list { background: #fff; border: 1px solid #e7eef0; border-radius: 14px; }
  .placeholder-metrics div { height: 113px; padding: 19px; }
  .placeholder-metrics i, .placeholder-metrics b, .placeholder-metrics span, .placeholder-list b, .placeholder-list i, .placeholder-list small, .placeholder-list em { display: block; background: #eff4f3; border-radius: 8px; }
  .placeholder-metrics i { width: 57px; height: 9px; }.placeholder-metrics b { width: 74px; height: 21px; margin-top: 15px; }.placeholder-metrics span { width: 44%; height: 7px; margin-top: 12px; }
  .placeholder-list { margin-top: 15px; padding: 21px 23px; }.placeholder-title { display: flex; justify-content: space-between; padding-bottom: 17px; }.placeholder-title b { width: 105px; height: 12px; }.placeholder-title i { width: 98px; height: 12px; }.placeholder-row { display: flex; gap: 12px; align-items: center; height: 55px; border-top: 1px solid #f1f5f4; }.placeholder-row > i { width: 27px; height: 27px; }.placeholder-row span { flex: 1; }.placeholder-row b { width: 160px; max-width: 80%; height: 9px; }.placeholder-row small { width: 90px; max-width: 60%; height: 7px; margin-top: 6px; }.placeholder-row em { width: 48px; height: 9px; }
  @media (max-width: 760px) { .index-card { padding: 17px; gap: 12px; }.placeholder-metrics { grid-template-columns: 1fr 1fr; }.placeholder-metrics div { height: 90px; }.progress-foot { flex-wrap: wrap; } }
  @media (prefers-reduced-motion: reduce) { .spinner, .track .indeterminate { animation: none; }.track span { transition: none; } }
</style>
