let nextId = 0;
let dismissActive: (() => void) | null = null;

/** One shared, unclipped tooltip surface for text and structured metric hints. */
export function openTooltip(
  anchor: Element,
  content: string | Node,
  { className = 'ui-tooltip', side = 'top', x, y }: { className?: string; side?: 'top' | 'right' | 'pointer'; x?: number; y?: number } = {},
) {
  dismissActive?.();
  const surface = document.createElement('div');
  surface.className = className;
  surface.id = `usage-tooltip-${++nextId}`;
  surface.role = 'tooltip';
  surface.popover = 'manual';
  const previous = anchor.getAttribute('aria-describedby');
  let closed = false;
  const replace = (value: string | Node) => surface.replaceChildren(value);
  replace(content);
  // Keep dialog tooltips in its accessible subtree; popover renders above all panels.
  (anchor.closest('dialog[open]') ?? document.body).append(surface);
  surface.showPopover?.();
  anchor.setAttribute('aria-describedby', [previous, surface.id].filter(Boolean).join(' '));

  function place(pointerX = x, pointerY = y) {
    if (closed) return;
    if (!anchor.isConnected) return close();
    // Content updates use the latest pointer position, including after pointermove.
    x = pointerX;
    y = pointerY;
    const rect = anchor.getBoundingClientRect();
    const box = surface.getBoundingClientRect();
    const gap = 10;
    let left = (pointerX ?? rect.left + rect.width / 2) - box.width / 2;
    let top = rect.top - box.height - gap;
    if (side === 'pointer' && pointerX !== undefined && pointerY !== undefined) {
      left = pointerX + 14;
      top = pointerY + 14;
      if (left + box.width > innerWidth - 8) left = pointerX - box.width - 14;
      if (top + box.height > innerHeight - 8) top = pointerY - box.height - 14;
    } else if (side === 'right') {
      left = rect.right + gap;
      top = rect.top + (rect.height - box.height) / 2;
      if (left + box.width > innerWidth - 8) left = rect.left - box.width - gap;
    } else if (top < 8) top = rect.bottom + gap;
    surface.style.left = `${Math.max(8, Math.min(innerWidth - box.width - 8, left))}px`;
    surface.style.top = `${Math.max(8, Math.min(innerHeight - box.height - 8, top))}px`;
  }
  function close() {
    if (closed) return;
    closed = true;
    surface.remove();
    const ids = (anchor.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(id => id && id !== surface.id);
    if (ids.length) anchor.setAttribute('aria-describedby', ids.join(' '));
    else anchor.removeAttribute('aria-describedby');
    if (dismissActive === close) dismissActive = null;
    window.removeEventListener('scroll', close, true);
    window.removeEventListener('resize', close);
    window.removeEventListener('blur', close);
    document.removeEventListener('keydown', key, true);
    document.removeEventListener('pointerdown', close, true);
  }
  function key(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (side === 'pointer') {
        event.preventDefault();
        event.stopPropagation();
      }
      close();
    }
  }
  dismissActive = close;
  window.addEventListener('scroll', close, true);
  window.addEventListener('resize', close);
  window.addEventListener('blur', close);
  document.addEventListener('keydown', key, true);
  document.addEventListener('pointerdown', close, true);
  place();
  return {
    close,
    place,
    update(value: string | Node) {
      if (closed) return;
      replace(value);
      place();
    },
  };
}

/** Delegation covers dynamically loaded rows, translated hints and icon controls. */
export function tooltips(root: HTMLElement) {
  let target: Element | null = null;
  let active: ReturnType<typeof openTooltip> | null = null;
  function text(element: Element) {
    if (element.hasAttribute('data-tooltip')) {
      const value = element.getAttribute('data-tooltip')?.trim() ?? '';
      // Names only need a full-text hint when ellipsis or line clamping hides text.
      if (element.hasAttribute('data-tooltip-overflow'))
        return element.scrollWidth > element.clientWidth + 1 || element.scrollHeight > element.clientHeight + 1 ? value : '';
      const normalize = (label: string) => label.replace(/\s+/g, '');
      return normalize(value) === normalize(element.textContent ?? '') ? '' : value;
    }
    const label = element.textContent?.trim();
    return (!label || label === '×') && !element.classList.contains('detail-backdrop')
      ? element.getAttribute('aria-label') ?? '' : '';
  }
  function find(node: EventTarget | null) {
    if (!(node instanceof Element)) return null;
    if (node.closest('[data-tooltip-managed]')) return null;
    const element = node.closest('[data-tooltip], button[aria-label], summary[aria-label]');
    return element && root.contains(element) && text(element) ? element : null;
  }
  function hide() {
    active?.close();
    active = null;
    target = null;
  }
  function enter(element: Element | null) {
    if (target === element) return;
    hide();
    if (!element) return;
    target = element;
    active = openTooltip(element, text(element), { side: element.closest('.sidebar') && innerWidth > 760 ? 'right' : 'top' });
  }
  const over = (event: PointerEvent) => { if (event.pointerType !== 'touch') enter(find(event.target)); };
  const out = (event: PointerEvent) => { if (find(event.relatedTarget) !== target) hide(); };
  const focus = (event: FocusEvent) => enter(find(event.target));
  const blur = (event: FocusEvent) => { if (find(event.relatedTarget) !== target) hide(); };
  const key = (event: KeyboardEvent) => { if (event.key === 'Escape') hide(); };
  const observer = new MutationObserver((records) => {
    if (!target) return;
    if (!target.isConnected || !text(target)) hide();
    else if (records.some(record => record.type === 'attributes' && record.target === target))
      active?.update(text(target));
  });
  observer.observe(root, { subtree: true, childList: true, attributes: true, attributeFilter: ['data-tooltip', 'data-tooltip-overflow', 'aria-label'] });
  root.addEventListener('pointerover', over);
  root.addEventListener('pointerout', out);
  root.addEventListener('focusin', focus);
  root.addEventListener('focusout', blur);
  root.addEventListener('keydown', key);
  root.addEventListener('pointerdown', hide);
  window.addEventListener('resize', hide);
  window.addEventListener('scroll', hide, true);
  return {
    destroy() {
      hide();
      observer.disconnect();
      root.removeEventListener('pointerover', over);
      root.removeEventListener('pointerout', out);
      root.removeEventListener('focusin', focus);
      root.removeEventListener('focusout', blur);
      root.removeEventListener('keydown', key);
      root.removeEventListener('pointerdown', hide);
      window.removeEventListener('resize', hide);
      window.removeEventListener('scroll', hide, true);
    },
  };
}
