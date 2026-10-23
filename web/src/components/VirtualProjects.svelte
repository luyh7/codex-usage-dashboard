<script lang="ts">
  import { onMount, tick, type Snippet } from 'svelte';
  import type { Project } from '../lib/data/types';

  let { projects, scope, active, more, loading, loadMore, row }: {
    projects: Project[];
    scope: string;
    active: boolean;
    more: boolean;
    loading: boolean;
    loadMore: () => void;
    row: Snippet<[Project]>;
  } = $props();
  let root: HTMLDivElement;
  let scroller: HTMLElement;
  let top = $state(0), viewport = $state(600), measured = $state(0);
  const heights = new Map<string, number>();
  let focused = $state('');
  let previousScope = '';
  let frame = 0;
  let observer: ResizeObserver;
  let offsets = $derived.by(() => {
    measured;
    const values = [0];
    for (const project of projects) values.push(values.at(-1)! + (heights.get(project.key) ?? 180));
    return values;
  });
  let visible = $derived.by(() => {
    const indices: number[] = [];
    for (let i = 0; i < projects.length; i++) {
      if ((offsets[i + 1] >= top - 500 && offsets[i] <= top + viewport + 500) || projects[i].key === focused)
        indices.push(i);
    }
    return indices;
  });
  function readViewport() {
    if (!active || !scroller) return;
    top = Math.max(0, scroller.scrollTop - root.offsetTop + scroller.offsetTop);
    viewport = scroller.clientHeight;
  }
  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(() => { frame = 0; readViewport(); });
  }
  function measure(node: HTMLElement, key: string) {
    node.dataset.projectKey = key;
    observer?.observe(node);
    return { destroy() { observer?.unobserve(node); } };
  }
  $effect(() => {
    if (scope !== previousScope) {
      previousScope = scope;
      heights.clear();
      measured++;
      top = 0;
      focused = '';
      if (scroller) scroller.scrollTop = 0;
    }
  });
  $effect(() => { if (active) void tick().then(readViewport); });
  $effect(() => {
    if (active && more && !loading && offsets.at(-1)! < top + viewport + 700) loadMore();
  });
  onMount(() => {
    scroller = root.closest('.browser-scroll') as HTMLElement;
    observer = new ResizeObserver(entries => {
      if (!active) return;
      let adjustment = 0, changed = false;
      for (const entry of entries) {
        const key = (entry.target as HTMLElement).dataset.projectKey!;
        const index = projects.findIndex(project => project.key === key);
        const height = entry.borderBoxSize[0]?.blockSize ?? entry.target.getBoundingClientRect().height;
        const previous = heights.get(key) ?? 180;
        if (height > 0 && Math.abs(height - previous) > 0.5) {
          if (index >= 0 && offsets[index + 1] < top) adjustment += height - previous;
          heights.set(key, height);
          changed = true;
        }
      }
      if (changed) {
        measured++;
        if (adjustment) scroller.scrollTop += adjustment;
        schedule();
      }
    });
    root.querySelectorAll<HTMLElement>('[data-project-key]').forEach(node => observer.observe(node));
    const resize = new ResizeObserver(schedule);
    resize.observe(scroller);
    scroller.addEventListener('scroll', schedule, { passive: true });
    readViewport();
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      resize.disconnect();
      scroller.removeEventListener('scroll', schedule);
    };
  });
</script>

<div bind:this={root} class="virtual-projects"
  onfocusin={(event) => focused = (event.target as HTMLElement).closest<HTMLElement>('[data-project-key]')?.dataset.projectKey ?? ''}
  onfocusout={(event) => { if (!root.contains(event.relatedTarget as Node)) focused = ''; }}>
  {#each visible as index, position (projects[index].key)}
    <div class="project-gap" style:height={`${offsets[index] - (position ? offsets[visible[position - 1] + 1] : 0)}px`} aria-hidden="true"></div>
    <div class="virtual-project" use:measure={projects[index].key}>
      {@render row(projects[index])}
    </div>
  {/each}
  <div class="project-gap" style:height={`${offsets.at(-1)! - (visible.length ? offsets[visible.at(-1)! + 1] : 0)}px`} aria-hidden="true"></div>
</div>

<style>
  .virtual-projects { overflow-anchor: none; }
  .virtual-project { display: flow-root; border-bottom: 1px solid var(--border); }
  .project-gap { pointer-events: none; }
</style>
