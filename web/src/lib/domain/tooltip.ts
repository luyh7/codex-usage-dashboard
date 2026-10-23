let nextId = 0;
let dismissActive: (() => void) | null = null;

type TooltipSide = 'top' | 'right' | 'pointer';
type Size = { width: number; height: number };
type TooltipOptions = { side?: TooltipSide; x?: number; y?: number };

/** Placement rules shared by text hints, structured hints, and chart readouts. */
export function tooltipPosition(
  anchor: Size & { left: number; top: number },
  surface: Size,
  viewport: Size,
  { side = 'top', x, y }: TooltipOptions = {},
) {
  const edge = 8, gap = 10, pointerGap = 14;
  let left = anchor.left + (anchor.width - surface.width) / 2;
  let top = anchor.top - surface.height - gap;
  if (side === 'pointer' && x !== undefined && y !== undefined) {
    left = x + pointerGap;
    top = y + pointerGap;
    if (left + surface.width > viewport.width - edge) left = x - surface.width - pointerGap;
    if (top + surface.height > viewport.height - edge) top = y - surface.height - pointerGap;
  } else if (side === 'right') {
    left = anchor.left + anchor.width + gap;
    top = anchor.top + (anchor.height - surface.height) / 2;
    if (left + surface.width > viewport.width - edge) left = anchor.left - surface.width - gap;
  } else if (top < edge) top = anchor.top + anchor.height + gap;
  return {
    left: Math.max(edge, Math.min(viewport.width - surface.width - edge, left)),
    top: Math.max(edge, Math.min(viewport.height - surface.height - edge, top)),
  };
}

/** Center numeric hints over their painted values rather than the whole column. */
function valueRect(anchor: Element) {
  const value = anchor.querySelector('.animated-value, .event-field-value') ?? anchor;
  const box = value.getBoundingClientRect();
  const range = document.createRange();
  range.selectNodeContents(value);
  const text = range.getBoundingClientRect();
  if (!text.width || !text.height) return box;
  const left = Math.max(box.left, text.left), right = Math.min(box.right, text.right);
  const top = Math.max(box.top, text.top), bottom = Math.min(box.bottom, text.bottom);
  return right > left && bottom > top ? { left, right, top, bottom, width: right - left, height: bottom - top } : box;
}

/** One shared, unclipped tooltip surface for text and structured metric hints. */
export function openTooltip(
  anchor: Element,
  content: string | Node,
  { className = 'ui-tooltip', side, x, y, reference = anchor, onClose }: TooltipOptions & { className?: string; reference?: Element; onClose?: () => void } = {},
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
    if (!anchor.isConnected || !reference.isConnected) return close();
    // Content updates use the latest pointer position, including after pointermove.
    x = pointerX;
    y = pointerY;
    const placement = side ?? (anchor.closest('.sidebar') && innerWidth > 760 ? 'right' : 'top');
    const rect = className === 'ui-tooltip' && placement === 'top' && !reference.matches('button, summary')
      ? valueRect(reference) : reference.getBoundingClientRect();
    const position = tooltipPosition(rect, surface.getBoundingClientRect(),
      { width: innerWidth, height: innerHeight },
      { side: placement, x, y });
    surface.style.left = `${position.left}px`;
    surface.style.top = `${position.top}px`;
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
    onClose?.();
  }
  function key(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
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
    let element = node.closest('[data-tooltip], button[aria-label], summary[aria-label]');
    while (element && root.contains(element)) {
      if (element.hasAttribute('data-tooltip')) return text(element) ? element : null;
      if (text(element)) return element;
      // A named button inside a numeric cell must not swallow the cell's hint.
      element = element.parentElement?.closest('[data-tooltip], button[aria-label], summary[aria-label]') ?? null;
    }
    return null;
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
    active = openTooltip(element, text(element));
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
    else if (records.some(record => record.type === 'childList' && target!.contains(record.target)))
      active?.place();
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
