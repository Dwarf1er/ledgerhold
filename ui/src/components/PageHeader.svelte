<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  let {
    title,
    back,
    subtitle,
    actions,
  }: { title: string; back?: { href: string; label: string }; subtitle?: Snippet; actions?: Snippet } = $props();
</script>

<header>
  <div class="text">
    {#if back}
      <a class="back" href={back.href}><Icon name="arrow-left" size={14} /> {back.label}</a>
    {/if}
    <h1>{title}</h1>
    {#if subtitle}<div class="sub muted">{@render subtitle()}</div>{/if}
  </div>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</header>

<style>
  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    margin-bottom: 24px;
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    font-size: 13px;
    color: var(--fg-3);
  }
  .back:hover {
    color: var(--fg);
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
    font-size: 14px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
