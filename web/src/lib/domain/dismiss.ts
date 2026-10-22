/** Close transient details on outside press or Escape without swallowing the next action. */
export function dismissDetails(node: HTMLDetailsElement) {
  const outside = (event: PointerEvent) => {
    if (node.open && event.target instanceof Node && !node.contains(event.target))
      node.open = false;
  };
  const keyboard = (event: KeyboardEvent) => {
    if (event.key !== 'Escape' || !node.open || document.querySelector('dialog[open]')) return;
    // Let the innermost dropdown dismiss before its enclosing preferences panel.
    const inner = event.target instanceof Element ? event.target.closest('details[open]') : null;
    if (inner && inner !== node && node.contains(inner)) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    node.open = false;
    node.querySelector('summary')?.focus({ preventScroll: true });
  };
  document.addEventListener('pointerdown', outside, true);
  document.addEventListener('keydown', keyboard, true);
  return {
    destroy() {
      document.removeEventListener('pointerdown', outside, true);
      document.removeEventListener('keydown', keyboard, true);
    },
  };
}

/** Native dialog backdrop clicks target the dialog; inner blank space must stay open. */
export function dismissBackdrop(node: HTMLDialogElement, dismiss: () => void) {
  let startedOutside = false;
  const outside = (event: MouseEvent) => {
    const box = node.getBoundingClientRect();
    return (
      event.target === node &&
      (event.clientX < box.left ||
        event.clientX > box.right ||
        event.clientY < box.top ||
        event.clientY > box.bottom)
    );
  };
  const down = (event: PointerEvent) => {
    startedOutside = outside(event);
  };
  const click = (event: MouseEvent) => {
    if (startedOutside && outside(event)) dismiss();
  };
  node.addEventListener('pointerdown', down);
  node.addEventListener('click', click);
  return {
    destroy() {
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('click', click);
    },
  };
}
