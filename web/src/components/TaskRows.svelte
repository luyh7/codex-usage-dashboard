<script lang="ts">
  import type { Session, TaskGroup } from '../lib/data/types';
  import { words, number, money, models, type Language } from '../lib/domain/ui';
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

<tr class:selected={selected === root.uid} class="task-row" data-uid={root.uid}>
  <td
    ><div class="title-cell">
      <button
        class="gutter"
        disabled={!children.length}
        aria-expanded={children.length ? expanded : undefined}
        aria-label={(expanded ? t.collapseAgents : t.expandAgents) + ': ' + root.title}
        onclick={toggle}>{children.length ? (expanded ? '▾' : '▸') : '·'}</button
      ><button class="session-title" onclick={() => select(root)} title={root.title}
        >{root.title || root.session_id}<small
          >{root.environment} · {models(root).join(', ')}{children.length
            ? ' · ' + children.length + ' agents'
            : ''}</small
        ></button
      >
    </div></td
  >
  <td class="numeric">{number(group.summary.usage.total_tokens, lang)}</td><td class="numeric"
    >{group.summary.estimated_cost_known_count
      ? money(group.summary.estimated_cost_usd, lang)
      : t.unknown}{#if group.summary.estimated_cost_known_count < group.sessions.length}<span
        title={t.unknownHint}
      >
        *</span
      >{/if}</td
  ><td class="numeric"
    >{number(
      group.sessions.reduce((n, s) => n + (s.turn_count || 0), 0),
      lang,
    )}</td
  >
</tr>
{#if expanded && children.length}
  <tr class="agent-row own" class:selected={selected === root.uid}
    ><td
      ><button class="session-title child-title" onclick={() => select(root)}>{t.ownUsage}</button
      ></td
    ><td class="numeric">{number(root.total_token_usage?.total_tokens, lang)}</td><td
      class="numeric">{money(root.estimated_cost_usd, lang)}</td
    ><td class="numeric">{number(root.turn_count, lang)}</td></tr
  >
  {#each children as row (row.uid)}<tr
      class="agent-row"
      class:selected={selected === row.uid}
      data-uid={row.uid}
      ><td
        ><button
          class="session-title child-title"
          style:padding-left={(1 + depth(row)) * 0.8 + 'rem'}
          onclick={() => select(row)}
          >{row.agent_nickname || row.agent_role || row.title || row.session_id}<small
            >{row.title} · {models(row).join(', ')}</small
          ></button
        ></td
      ><td class="numeric">{number(row.total_token_usage?.total_tokens, lang)}</td><td
        class="numeric">{money(row.estimated_cost_usd, lang)}</td
      ><td class="numeric">{number(row.turn_count, lang)}</td></tr
    >{/each}
{/if}
