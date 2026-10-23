import { mount, unmount, tick } from 'svelte';
import SessionMetadata from '../../components/SessionMetadata.svelte';
import type { Language } from './ui';
import { openTooltip } from './tooltip';

export interface ModelTagData {
  model: string;
  effort: string;
  service_tier: string;
  longContext?: boolean;
  lang: Language;
}

export interface MetricTooltipData {
  title: string;
  rows: { label: string; value: string }[];
  tables?: {
    caption: string;
    modelTags?: ModelTagData;
    period?: string;
    columns: string[];
    rows: { label: string; values: string[]; color?: string; nested?: boolean }[];
  }[];
  note?: string;
  followPointer?: boolean;
  className?: string;
}

/** Structured chart hints use the same surface and lifecycle as text hints. */
export function metricTooltip(node: HTMLElement | SVGElement, initial: MetricTooltipData) {
  let data = initial;
  let active: ReturnType<typeof openTooltip> | null = null;
  const mounted: ReturnType<typeof mount>[] = [];
  function clearMounted() {
    for (const component of mounted.splice(0)) void unmount(component);
  }
  // The delegated text hints must never take over this structured hint on aria updates.
  node.setAttribute('data-tooltip-managed', '');
  function content() {
    const fragment = document.createDocumentFragment();
    if (data.title) {
      const title = document.createElement('div');
      title.className = 'metric-tooltip-title';
      title.textContent = data.title;
      fragment.append(title);
    }
    for (const row of data.rows) {
      const line = document.createElement('div');
      line.className = 'metric-tooltip-row';
      const label = document.createElement('span');
      label.textContent = row.label;
      const value = document.createElement('b');
      value.textContent = row.value;
      line.append(label, value);
      fragment.append(line);
    }
    for (const section of data.tables ?? []) {
      const table = document.createElement('table');
      table.className = 'tooltip-table';
      const caption = table.createCaption();
      if (section.modelTags) {
        const tags = section.modelTags;
        mounted.push(mount(SessionMetadata, {
          target: caption,
          props: {
            sessions: [{ ...tags, models: [tags.model] }],
            lang: tags.lang,
            longContext: tags.longContext,
          },
        }));
      } else caption.textContent = section.caption;
      if (section.period) {
        const period = document.createElement('div');
        period.className = 'tooltip-price-period';
        period.textContent = section.period;
        caption.append(period);
      }
      const heading = table.createTHead().insertRow();
      for (const label of section.columns) {
        const cell = document.createElement('th');
        cell.scope = 'col';
        cell.textContent = label;
        heading.append(cell);
      }
      const body = table.createTBody();
      for (const row of section.rows) {
        const line = body.insertRow();
        if (row.nested) line.className = 'tooltip-subrow';
        const label = document.createElement('th');
        label.scope = 'row';
        if (row.color && !row.nested) {
          const dot = document.createElement('i');
          dot.style.background = row.color;
          label.append(dot);
        }
        label.append(row.label);
        line.append(label);
        for (const value of row.values) line.insertCell().textContent = value;
      }
      fragment.append(table);
    }
    if (data.note) {
      const note = document.createElement('div');
      note.className = 'tooltip-note';
      note.textContent = data.note;
      fragment.append(note);
    }
    return fragment;
  }
  function show(event: Event) {
    if (event instanceof PointerEvent && event.pointerType === 'touch') return;
    hide();
    active = openTooltip(node, content(), {
      className: data.className ?? 'metric-tooltip',
      x: data.followPointer && event instanceof PointerEvent ? event.clientX : undefined,
      y: data.followPointer && event instanceof PointerEvent ? event.clientY : undefined,
      side: data.followPointer ? 'pointer' : 'top',
      onClose: () => { active = null; clearMounted(); },
    });
    void tick().then(() => active?.place());
  }
  function hide() { active?.close(); active = null; clearMounted(); }
  function move(event: Event) {
    if (data.followPointer) active?.place((event as PointerEvent).clientX, (event as PointerEvent).clientY);
  }
  const listeners: [string, EventListener][] = [
    ['pointerenter', show], ['pointermove', move], ['pointerleave', hide],
    ['focus', show], ['blur', hide],
  ];
  for (const [name, listener] of listeners) node.addEventListener(name, listener);
  return {
    update(next: MetricTooltipData) {
      data = next;
      if (active) {
        clearMounted();
        active.update(content());
        void tick().then(() => active?.place());
      }
    },
    destroy() {
      hide();
      node.removeAttribute('data-tooltip-managed');
      for (const [name, listener] of listeners) node.removeEventListener(name, listener);
    },
  };
}
