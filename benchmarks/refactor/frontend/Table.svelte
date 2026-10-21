<script>
  // Probe: reuse production markup and grouping; isolate keyed DOM updates.
  let blocks = $state([]);
  export function update(next) { blocks = next; }
</script>

{#each blocks as block (block.key)}
  {@html block.html}
{/each}
