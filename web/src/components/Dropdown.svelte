<script module lang="ts">
  let nextId = 0;
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import { dismissDetails } from '../lib/domain/dismiss';
  let {
    value,
    options,
    label,
    change,
    variant = 'field',
    display,
    searchLabel,
    emptyLabel = '',
    class: className = '',
  }: {
    value: string;
    options: readonly (readonly [string, string])[];
    label: string;
    change: (value: string) => void;
    variant?: 'field' | 'sort' | 'text';
    display?: string;
    searchLabel?: string;
    emptyLabel?: string;
    class?: string;
  } = $props();
  const id = `dropdown-${++nextId}`;
  let menu: HTMLDetailsElement;
  let popup: HTMLDivElement;
  let open = $state(false);
  let searchInput = $state<HTMLInputElement>();
  let filter = $state('');
  let visibleOptions = $derived(options.filter(([key, text]) =>
    `${key} ${key.replaceAll('_', ' ')} ${text}`.toLocaleLowerCase().includes(filter.trim().toLocaleLowerCase()),
  ));
  let search = '',
    searchAt = 0;
  let isMenu = $derived(variant !== 'field');
  function positionPopup() {
    if (!popup?.matches(':popover-open')) return;
    const anchor = menu.getBoundingClientRect();
    popup.style.width = `${Math.min(innerWidth - 16, Math.max(184, variant === 'field' ? anchor.width : 0))}px`;
    const below = innerHeight - anchor.bottom - 16;
    const above = anchor.top - 16;
    const down = below >= Math.min(popup.scrollHeight, 280) || below >= above;
    popup.style.maxHeight = `${Math.max(40, Math.min(280, down ? below : above))}px`;
    const box = popup.getBoundingClientRect();
    popup.style.left = `${Math.max(8, Math.min(innerWidth - box.width - 8, variant === 'sort' ? anchor.right - box.width : anchor.left))}px`;
    popup.style.top = `${Math.max(8, Math.min(innerHeight - box.height - 8, down ? anchor.bottom + 8 : anchor.top - box.height - 8))}px`;
  }
  function syncPopup() {
    if (menu.open) {
      if (!popup.matches(':popover-open')) {
        popup.showPopover();
        if (searchLabel) searchInput?.focus({ preventScroll: true });
      }
      positionPopup();
    } else if (popup.matches(':popover-open')) {
      popup.hidePopover();
      filter = '';
    }
  }
  function close(restore = false) {
    menu.open = false;
    filter = '';
    syncPopup();
    if (restore) menu.querySelector('summary')?.focus({ preventScroll: true });
  }
  function choose(key: string) {
    close(true);
    change(key);
  }
  function keyboard(node: HTMLDetailsElement) {
    const key = async (event: KeyboardEvent) => {
      if (event.key === 'Escape' && node.open) {
        event.preventDefault();
        event.stopPropagation();
        close(true);
        return;
      }
      const navigation = ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key);
      if (event.target === searchInput && !['ArrowDown', 'ArrowUp'].includes(event.key)) return;
      const typing =
        event.key.length === 1 &&
        event.key !== ' ' &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey;
      if (!navigation && !typing) return;
      event.preventDefault();
      const wasOpen = node.open;
      node.open = true;
      await tick();
      syncPopup();
      const buttons = [...node.querySelectorAll<HTMLButtonElement>('[data-option]')];
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      let next =
        event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? buttons.length - 1
            : !wasOpen || index < 0
              ? event.key === 'ArrowUp'
                ? buttons.length - 1
                : 0
              : (index + (event.key === 'ArrowUp' ? -1 : 1) + buttons.length) % buttons.length;
      if (typing) {
        search = Date.now() - searchAt < 700 ? search + event.key : event.key;
        searchAt = Date.now();
        next = visibleOptions.findIndex(([, label]) =>
          label.toLocaleLowerCase().startsWith(search.toLocaleLowerCase()),
        );
      }
      buttons[next]?.focus({ preventScroll: true });
      buttons[next]?.scrollIntoView({ block: 'nearest' });
    };
    const blur = (event: FocusEvent) => {
      if (event.relatedTarget instanceof Node && !node.contains(event.relatedTarget)) close();
    };
    node.addEventListener('keydown', key);
    node.addEventListener('focusout', blur);
    node.addEventListener('toggle', syncPopup);
    window.addEventListener('resize', positionPopup);
    window.addEventListener('scroll', positionPopup, true);
    return {
      destroy() {
        node.removeEventListener('keydown', key);
        node.removeEventListener('focusout', blur);
        node.removeEventListener('toggle', syncPopup);
        window.removeEventListener('resize', positionPopup);
        window.removeEventListener('scroll', positionPopup, true);
      },
    };
  }
</script>

<details
  class="dropdown {className}"
  class:compact={variant === 'sort'}
  class:text={variant === 'text'}
  bind:this={menu}
  bind:open
  use:dismissDetails
  use:keyboard
>
  <summary
    aria-label={label}
    aria-haspopup={isMenu ? 'menu' : 'listbox'}
    aria-expanded={open}
    aria-controls={id}
  >
    <span>{display ?? options.find(([key]) => key === value)?.[1] ?? value}</span><Icon
      name="down"
      size={13}
    />
  </summary>
  <div
    bind:this={popup}
    {id}
    popover="manual"
    class="dropdown-options"
    class:sort-options={variant === 'sort'}
    role={isMenu ? 'menu' : 'listbox'}
    aria-label={label}
  >
    {#if searchLabel}
      <div class="dropdown-search">
        <input bind:this={searchInput!} bind:value={filter} aria-label={searchLabel} placeholder={searchLabel} autocomplete="off" spellcheck="false" />
      </div>
    {/if}
    {#each visibleOptions as [key, text] (key)}
      <button
        type="button"
        data-option={key}
        role={isMenu ? 'menuitemradio' : 'option'}
        aria-checked={isMenu ? value === key : undefined}
        aria-selected={!isMenu ? value === key : undefined}
        class:selected={value === key}
        onclick={() => choose(key)}
      >
        <span>{text}</span>{#if value === key}<Icon name="check" size={15} />{/if}
      </button>
    {/each}
    {#if !visibleOptions.length}<div class="dropdown-empty" role="status">{emptyLabel}</div>{/if}
  </div>
</details>

<style>
  .dropdown {
    min-width: 0;
    width: 100%;
  }
  summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    min-height: 34px;
    padding: 7px 10px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--panel);
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
    list-style: none;
  }
  summary > span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  summary :global(.ui-icon) {
    flex-shrink: 0;
    color: var(--muted);
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary:hover {
    background: var(--hover);
  }
  summary:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .compact {
    width: auto;
  }
  .compact summary {
    min-width: 112px;
    min-height: 36px;
    padding: 7px 10px 7px 0;
    border: 0;
    border-radius: 0 8px 8px 0;
  }
  .text summary {
    border: 0;
    background: transparent;
  }
  .dropdown-options {
    position: fixed;
    inset: auto;
    margin: 0;
    overflow: auto;
    overscroll-behavior: contain;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 11px;
    box-shadow: 0 8px 28px #16263f18;
    background: var(--panel);
    color: var(--text);
  }
  .dropdown-options button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    min-height: 36px;
    padding: 8px 10px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    font-size: 12px;
    text-align: left;
  }
  .dropdown-search {
    position: sticky;
    top: -5px;
    z-index: 1;
    margin: -5px -5px 5px;
    padding: 8px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .dropdown-search input { width: 100%; min-width: 0; }
  .dropdown-empty { padding: 12px; color: var(--muted); font-size: 12px; }
  .dropdown-options button > span {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .dropdown-options button :global(.ui-icon) {
    flex-shrink: 0;
  }
  .dropdown-options button:hover,
  .dropdown-options button:focus-visible {
    background: var(--hover);
  }
  .dropdown-options button.selected {
    color: var(--accent-strong);
    background: var(--accent-bg);
    font-weight: 600;
  }
  @media (max-width: 760px) {
    summary {
      font-size: 11px;
    }
    .compact summary {
      min-width: 90px;
      gap: 6px;
    }
  }
</style>
