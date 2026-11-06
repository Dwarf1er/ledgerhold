<script lang="ts">
  import type { Snippet } from 'svelte';
  let {
    items,
    value = $bindable(),
    label,
    size = 'md',
    onchange,
  }: {
    items: { id: string; label?: string; icon?: Snippet; title?: string }[];
    value: string;
    label: string;
    size?: 'sm' | 'md';
    onchange?: (id: string) => void;
  } = $props();

  function pick(id: string) {
    value = id;
    onchange?.(id);
  }
</script>

<div class="seg {size}" role="radiogroup" aria-label={label}>
  {#each items as it (it.id)}
    <button
      type="button"
      role="radio"
      aria-checked={value === it.id}
      aria-label={it.title ?? it.label}
      title={it.title}
      class:on={value === it.id}
      onclick={() => pick(it.id)}
    >
      {#if it.icon}{@render it.icon()}{/if}
      {#if it.label}{it.label}{/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    background: var(--bg-3);
    border-radius: 8px;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--fg-3);
    font-size: 13px;
    font-weight: 500;
  }
  .sm button {
    height: 24px;
    padding: 0 8px;
  }
  button:hover {
    color: var(--fg);
  }
  .on {
    background: var(--bg);
    color: var(--fg);
    box-shadow: 0 0 0 1px var(--line), 0 1px 2px rgb(0 0 0 / 0.06);
  }
</style>
