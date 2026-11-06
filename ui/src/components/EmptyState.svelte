<script lang="ts">
  import type { Snippet } from 'svelte';
  let { title, children, action }: { title: string; children?: Snippet; action?: Snippet } = $props();
</script>

<div class="empty">
  <h2>{title}</h2>
  {#if children}<p class="muted">{@render children()}</p>{/if}
  {#if action}<div class="act">{@render action()}</div>{/if}
</div>

<style>
  .empty {
    display: grid;
    justify-items: center;
    gap: 6px;
    padding: 48px 24px;
    text-align: center;
    border: 1px dashed var(--line-2);
    border-radius: var(--radius-lg);
    background: var(--bg-2);
  }
  p {
    max-width: 44ch;
  }
  .act {
    margin-top: 12px;
  }
</style>
