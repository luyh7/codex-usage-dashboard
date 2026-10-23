<script lang="ts">
  import type { Session, TaskGroup } from '../lib/data/types';
  import { words, number, money, type Language } from '../lib/domain/ui';
  import Icon from './Icon.svelte';
  import AnimatedValue from './AnimatedValue.svelte';
  import { compactNumber, displayMoney, activityTime, latestActivityTime } from '../lib/domain/presentation';
  import RelativeTime from './RelativeTime.svelte';
  import ArchiveTag from './ArchiveTag.svelte';
  import SessionMetadata from './SessionMetadata.svelte';
  import CacheHit from './CacheHit.svelte';
  import { usageSpeedHint } from '../lib/domain/speed';
  let {
    group,
    selected,
    expanded,
    toggle,
    select,
    lang,
    zone,
  }: {
    group: TaskGroup;
    selected: string;
    expanded: boolean;
    toggle: () => void;
    select: (row: Session) => void;
    lang: Language;
    zone: string;
  } = $props();
  let t = $derived(words(lang));
  let root = $derived(
    group.sessions.find((s) => !s.is_subagent) ||
      group.sessions.find(
        (s) => !group.sessions.some((p) => p.session_id === s.parent_thread_id),
      ) ||
      group.sessions[0],
  );
  let children = $derived(group.sessions.filter((s) => s !== root));
  function depth(row: Session) {
    let level = 0;
    let parent = row.parent_thread_id;
    const seen = new Set<string>();
    while (parent && !seen.has(parent)) {
      seen.add(parent);
      const found = group.sessions.find((s) => s.session_id === parent);
      if (!found) break;
      level++;
      parent = found.parent_thread_id;
    }
    return Math.min(level, 8);
  }
</script>

{#if root}
  <div class="task-group" class:has-children={children.length > 0}>
    <button
      class="project-row task-row"
      class:group-row={children.length > 0}
      class:selected={!children.length && selected === root.uid}
      data-uid={children.length ? undefined : root.uid}
      aria-expanded={children.length ? expanded : undefined}
      aria-pressed={children.length ? undefined : selected === root.uid}
      onclick={() => (children.length ? toggle() : select(root))}
    >
      <span class="tree-link" aria-hidden="true"></span>
      <span class="task-name"
        ><span class="gutter"
          >{#if children.length}<span class:expanded><Icon name="chevron" size={14} /></span
            >{/if}</span
        ><Icon name="message" size={24} /><span class="session-title"
          ><span class="task-title-line"
            ><ArchiveTag source={root.source} {lang} /><span class="title-text" data-tooltip={root.title} data-tooltip-overflow>{root.title || root.session_id}</span
            ><RelativeTime value={latestActivityTime(group.sessions)} {lang} {zone} /></span
          >{#if children.length}<small
              >{t.mainThread} + <AnimatedValue
                value={children.length}
                format={(value) => number(Math.round(value), lang)}
              />
              {t.subagents}</small
            >{/if}<SessionMetadata sessions={group.sessions} {lang} /></span
        ></span
      >
      <span class="numeric" data-tooltip={usageSpeedHint(group.summary.estimated_speed, lang)}
        ><AnimatedValue value={group.summary.usage.total_tokens} format={compactNumber} /></span
      >
      <span class="numeric cost-cell"
        >{#if group.summary.estimated_cost_known_count}<AnimatedValue
            value={group.summary.estimated_cost_usd}
            format={displayMoney}
          />{:else}{t.unknown}{/if}{#if group.summary.estimated_cost_known_count < group.sessions.length}<span
            data-tooltip={t.unknownHint}>*</span
          >{/if}</span
      >
      <span class="numeric cache-cell"><CacheHit usage={group.summary.usage} {lang} /></span>
    </button>
    {#if expanded && children.length}
      <div class="agent-children">
        <button
          class="project-row agent-row own"
          class:selected={selected === root.uid}
          data-uid={root.uid}
          aria-pressed={selected === root.uid}
          onclick={() => select(root)}
        >
          <span class="tree-link" aria-hidden="true"></span>
          <span class="task-name"
            ><Icon name="message" size={24} /><span class="session-title"
              ><span class="task-title-line"><ArchiveTag source={root.source} {lang} /><span class="title-text" data-tooltip={root.title} data-tooltip-overflow>{root.title || root.session_id}</span><span class="role-tag main-role-tag">{t.mainThread}</span><RelativeTime value={activityTime(root)} {lang} {zone} /></span><SessionMetadata sessions={[root]} {lang} /></span
            ></span
          >
          <span class="numeric" data-tooltip={usageSpeedHint(root.estimated_speed, lang)}
            ><AnimatedValue
              value={root.total_token_usage?.total_tokens ?? 0}
              format={compactNumber}
            /></span
          ><span class="numeric cost-cell" data-tooltip={money(root.estimated_cost_usd, lang)}
            >{#if root.estimated_cost_usd == null}{t.unknown}{:else}<AnimatedValue
                value={root.estimated_cost_usd}
                format={displayMoney}
              />{/if}</span
          >
          <span class="numeric cache-cell"><CacheHit usage={root.total_token_usage} {lang} /></span>
        </button>
        {#each children as row (row.uid)}<button
            class="project-row agent-row"
            class:selected={selected === row.uid}
            data-uid={row.uid}
            aria-pressed={selected === row.uid}
            onclick={() => select(row)}
          >
            <span class="tree-link" aria-hidden="true"></span>
            <span class="task-name" style:--agent-depth={Math.max(0, depth(row) - 1)}
              ><Icon name="message" size={24} /><span class="session-title"
                ><span class="task-title-line"
                  ><ArchiveTag source={row.source} {lang} /><span class="title-text" data-tooltip={row.title} data-tooltip-overflow
                    >{row.agent_nickname || row.agent_role || row.title || row.session_id}</span
                  ><span class="role-tag">{t.agentLabel}</span><RelativeTime value={activityTime(row)} {lang} {zone} /></span
                ><SessionMetadata sessions={[row]} {lang} /></span
              ></span
            >
            <span class="numeric" data-tooltip={usageSpeedHint(row.estimated_speed, lang)}
              ><AnimatedValue
                value={row.total_token_usage?.total_tokens ?? 0}
                format={compactNumber}
              /></span
            ><span class="numeric cost-cell" data-tooltip={money(row.estimated_cost_usd, lang)}
              >{#if row.estimated_cost_usd == null}{t.unknown}{:else}<AnimatedValue
                  value={row.estimated_cost_usd}
                  format={displayMoney}
                />{/if}</span
            >
            <span class="numeric cache-cell"><CacheHit usage={row.total_token_usage} {lang} /></span>
          </button>{/each}
      </div>
    {/if}
  </div>
{/if}
