<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  let { tone = 'info', children }: { tone?: 'info' | 'warn' | 'error' | 'success'; children: Snippet } = $props();
</script>

<div class="alert {tone}" role={tone === 'error' ? 'alert' : 'note'}>
  <span class="ico"><Icon name={tone === 'error' || tone === 'warn' ? 'alert' : tone === 'success' ? 'check' : 'info'} size={16} /></span>
  <div>{@render children()}</div>
</div>

<style>
  .alert {
    display: flex;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--line);
    background: var(--bg-2);
    font-size: 13px;
    color: var(--fg-2);
  }
  .ico {
    display: flex;
    margin-top: 1px;
  }
  .warn {
    background: var(--amber-bg);
    color: var(--amber);
    border-color: transparent;
  }
  .error {
    background: var(--red-bg);
    color: var(--red);
    border-color: transparent;
  }
  .success {
    background: var(--green-bg);
    color: var(--green);
    border-color: transparent;
  }
  .info .ico {
    color: var(--fg-3);
  }
</style>
