# UI Contract

Applies to all UI changes. [Clear Mint](../design/clear-mint/README.md) provides visual references; this file defines the UI requirements.

## Presentation

- Avoid unnecessary secondary labels and descriptions. Let the layout explain the interface.
- Buttons need clear fill or borders and distinct hover, focus, pressed, and disabled states.
- Format dates and times in the selected display timezone. Show timezone names, abbreviations, and offsets only in the timezone setting, including for tooltips.
- Use 会话 / session, 主会话 / Main thread, 子代理 / Subagent, and 会话组 / session group consistently.

## Shared controls

- Reuse `Dropdown.svelte` for selects, with keyboard navigation, search where applicable, Escape/outside dismissal, and viewport-safe popovers.
- Keep calendar edits as drafts until Apply; cancel, outside click, and Escape discard them without changing the query.

## Tooltips

- Use `data-tooltip` / `openTooltip` with shared appearance in `web/src/tooltip.css`. No native HTML `title` hints or separate tooltip surfaces/styles; structured contents may differ.
- Ordinary controls and text: center above the target, flip below when space above is insufficient. Anchor to the target, not the pointer.
- Left sidebar navigation: show to the right, falling back to the left if needed. When navigation becomes a horizontal bar, use the ordinary rule.
- Chart readouts: follow the pointer; keyboard inspection anchors to the selected data point. Flip at viewport edges. Keep a 10px target gap, 14px pointer gap, and at least 8px viewport clearance.
- Use the same hint on hover and keyboard focus, connect it with `aria-describedby`, and close on leave, blur, Escape, click, scroll, or resize. Escape dismisses the visible hint before its surrounding panel. Render above panels without clipping.

## Verification

- Check changed controls in the running worktree dashboard: default, hover, focus, pressed, and disabled states where applicable.
- For hints, verify placement, dismissal, and viewport clearance on desktop and narrow screens. Save a screenshot with the changed hint open; a default-state screenshot is insufficient.
