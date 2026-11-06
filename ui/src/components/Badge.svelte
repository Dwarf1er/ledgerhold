<script lang="ts">
  import type { Snippet } from 'svelte';
  let { tone = 'neutral', children }: { tone?: 'neutral' | 'green' | 'red' | 'amber' | 'blue'; children: Snippet } = $props();
</script>

<span class="badge {tone}">{@render children()}</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
    border: 1px solid var(--line);
    background: var(--bg-2);
    color: var(--fg-2);
  }
  .green {
    background: var(--green-bg);
    color: var(--green);
    border-color: transparent;
  }
  .red {
    background: var(--red-bg);
    color: var(--red);
    border-color: transparent;
  }
  .amber {
    background: var(--amber-bg);
    color: var(--amber);
    border-color: transparent;
  }
  .blue {
    background: var(--accent-bg);
    color: var(--accent);
    border-color: transparent;
  }
</style>
