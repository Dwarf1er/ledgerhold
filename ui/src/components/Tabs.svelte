<script lang="ts">
  let {
    items,
    value,
    onselect,
  }: {
    items: { id: string; label: string; href?: string }[];
    value: string;
    onselect?: (id: string) => void;
  } = $props();
</script>

<div class="tabs" role="tablist">
  {#each items as it (it.id)}
    {#if it.href}
      <a href={it.href} role="tab" aria-selected={value === it.id} class:active={value === it.id}>{it.label}</a>
    {:else}
      <button type="button" role="tab" aria-selected={value === it.id} class:active={value === it.id} onclick={() => onselect?.(it.id)}>
        {it.label}
      </button>
    {/if}
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    gap: 20px;
    border-bottom: 1px solid var(--line);
  }
  a,
  button {
    position: relative;
    padding: 10px 0;
    margin-bottom: -1px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--fg-3);
    font-weight: 500;
    font-size: 14px;
  }
  a:hover,
  button:hover {
    color: var(--fg);
  }
  .active {
    color: var(--fg);
    border-bottom-color: var(--fg);
  }
</style>
