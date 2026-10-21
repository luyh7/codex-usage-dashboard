<script lang="ts">
  import type { Session, TaskGroup } from '../lib/data/types';
  import { words, number, money, models, type Language } from '../lib/domain/ui';
  import Icon from './Icon.svelte';
  import { compactNumber, displayMoney } from '../lib/domain/presentation';
  let {
    group,
    selected,
    expanded,
    toggle,
    select,
    lang,
  }: {
    group: TaskGroup;
    selected: string;
    expanded: boolean;
    toggle: () => void;
    select: (row: Session) => void;
    lang: Language;
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
      <span class="task-name"
        ><span class="gutter"
          >{#if children.length}<span class:expanded><Icon name="chevron" size={14} /></span
            >{/if}</span
        ><Icon name="message" size={24} /><span class="session-title"
          ><span class="task-title-line"
            ><span class="title-text" title={root.title}>{root.title || root.session_id}</span
            >{#if children.length}<span class="role-tag">{t.taskTotal}</span>{/if}</span
          >{#if children.length}<small>{t.mainTask} + {children.length} {t.subagents}</small
            >{/if}</span
        ></span
      >
      <span class="numeric" title={number(group.summary.usage.total_tokens, lang)}
        >{compactNumber(group.summary.usage.total_tokens)}</span
      >
      <span class="numeric cost-cell"
        >{group.summary.estimated_cost_known_count
          ? displayMoney(group.summary.estimated_cost_usd)
          : t.unknown}{#if group.summary.estimated_cost_known_count < group.sessions.length}<span
            title={t.unknownHint}>*</span
          >{/if}</span
      >
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
          <span class="task-name"
            ><Icon name="message" size={24} /><span class="session-title"
              ><span class="title-text">{t.ownUsage}</span><small title={root.title}
                >{root.title || root.session_id}</small
              ></span
            ></span
          >
          <span class="numeric" title={number(root.total_token_usage?.total_tokens, lang)}
            >{compactNumber(root.total_token_usage?.total_tokens)}</span
          ><span class="numeric cost-cell" title={money(root.estimated_cost_usd, lang)}
            >{root.estimated_cost_usd == null
              ? t.unknown
              : displayMoney(root.estimated_cost_usd)}</span
          >
        </button>
        {#each children as row (row.uid)}<button
            class="project-row agent-row"
            class:selected={selected === row.uid}
            data-uid={row.uid}
            aria-pressed={selected === row.uid}
            onclick={() => select(row)}
          >
            <span class="task-name" style:--agent-depth={Math.max(0, depth(row) - 1)}
              ><Icon name="message" size={24} /><span class="session-title"
                ><span class="task-title-line"
                  ><span class="title-text" title={row.title}
                    >{row.agent_nickname || row.agent_role || row.title || row.session_id}</span
                  ><span class="role-tag">{t.agentLabel}</span></span
                ></span
              ></span
            >
            <span class="numeric" title={number(row.total_token_usage?.total_tokens, lang)}
              >{compactNumber(row.total_token_usage?.total_tokens)}</span
            ><span class="numeric cost-cell" title={money(row.estimated_cost_usd, lang)}
              >{row.estimated_cost_usd == null
                ? t.unknown
                : displayMoney(row.estimated_cost_usd)}</span
            >
          </button>{/each}
      </div>
    {/if}
  </div>
{/if}
