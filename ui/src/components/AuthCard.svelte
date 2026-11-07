<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, onsubmit, children }: { title: string; onsubmit: (e: SubmitEvent) => void; children: Snippet } = $props();
</script>

<div class="wrap">
  <form class="card" {onsubmit}>
    <div class="brand"><span class="mark" aria-hidden="true"></span> Ledgerhold</div>
    <h1>{title}</h1>
    {@render children()}
  </form>
</div>

<style>
  .wrap {
    display: grid;
    place-items: center;
    min-height: 100%;
    padding: 24px;
  }
  form {
    display: grid;
    gap: 14px;
    width: 100%;
    max-width: 380px;
    padding: 28px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .mark {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    background: var(--fg);
    position: relative;
  }
  .mark::after {
    content: '';
    position: absolute;
    left: 4px;
    right: 4px;
    top: 5px;
    height: 2px;
    background: var(--bg);
    box-shadow: 0 4px 0 var(--bg);
  }
  h1 {
    font-size: 20px;
    line-height: 28px;
  }
</style>
