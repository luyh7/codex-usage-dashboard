<script lang="ts">
  let { name, size = 20 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    lightning: 'm13 2-9 12h7l-1 8 10-12h-7l1-8Z',
    check: 'm5 12 4 4L19 6',
    copy: 'M9 9h12v12H9ZM15 9V3H3v12h6',
    overview: 'M5 20v-6M12 20V5M19 20V10',
    folder: 'M2 7V5a2 2 0 0 1 2-2h5l2.5 3H20a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V7Z',
    devices: 'M3 6c0-5 18-5 18 0s-18 5-18 0Zm0 0v12c0 5 18 5 18 0V6M3 12c0 5 18 5 18 0',
    settings:
      'M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1 1-3ZM12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z',
    search: 'M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15ZM16 16l5 5',
    filter: 'M3 4h18l-7 8v7l-4 2v-9L3 4Z',
    calendar:
      'M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2ZM8 2v5M16 2v5M3 10h18',
    chevron: 'm9 5 7 7-7 7',
    down: 'm6 9 6 6 6-6',
    close: 'm6 6 12 12M18 6 6 18',
    message:
      'M4 2.5h16a1.5 1.5 0 0 1 1.5 1.5v13a1.5 1.5 0 0 1-1.5 1.5H8l-5.5 4V4A1.5 1.5 0 0 1 4 2.5ZM6.5 7h11M6.5 11h8',
    events:
      'M6 2.5h12a2 2 0 0 1 2 2v15a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-15a2 2 0 0 1 2-2ZM8 7h8M8 11h8M8 15h5',
    sort: 'M7 3v18m-4-4 4 4 4-4M17 21V3m-4 4 4-4 4 4',
    'sort-descending': 'M3 6h9M3 12h6M3 18h3M18 4v16m-4-4 4 4 4-4',
    'sort-ascending': 'M3 6h3M3 12h6M3 18h9M18 20V4m-4 4 4-4 4 4',
    wallet: 'M20 7H5a2 2 0 0 1 0-4h13v4M3 5v14a2 2 0 0 0 2 2h15V7M20 11h-5v6h5M16 14h1',
    tokens: 'm12 3 9 5-9 5-9-5 9-5ZM3 12l9 5 9-5M3 16l9 5 9-5',
    shield: 'm12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Zm-4 9 3 3 5-6',
    arrow: 'M12 4v16m-6-6 6 6 6-6',
    import: 'M12 16V3m-5 8 5 5 5-5M4 16v5h16v-5',
    export: 'M12 3v13m-5-8 5-5 5 5M4 16v5h16v-5',
    info: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM12 11v6M12 7v1',
    'alert-circle': 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM12 7v6M12 16v.1',
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  data-icon={name}
  class="ui-icon"><path d={paths[name] || paths.message} /></svg
>
