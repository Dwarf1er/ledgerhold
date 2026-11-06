<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    variant = 'secondary',
    size = 'md',
    type = 'button',
    disabled = false,
    loading = false,
    href,
    onclick,
    children,
    ...rest
  }: {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'sm' | 'md';
    type?: 'button' | 'submit';
    disabled?: boolean;
    loading?: boolean;
    href?: string;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
    [k: string]: unknown;
  } = $props();
</script>

{#if href && !disabled}
  <a {href} class="btn {variant} {size}" {...rest}>{@render children()}</a>
{:else}
  <button {type} class="btn {variant} {size}" disabled={disabled || loading} {onclick} {...rest}>
    {#if loading}<span class="spin" aria-hidden="true"></span>{/if}
    {@render children()}
  </button>
{/if}

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px;
    border-radius: var(--radius);
    border: 1px solid var(--line-2);
    background: var(--bg);
    color: var(--fg);
    font-weight: 500;
    font-size: 14px;
    line-height: 1;
    white-space: nowrap;
    transition: background 0.12s, border-color 0.12s, opacity 0.12s;
  }
  .sm {
    height: 28px;
    padding: 0 10px;
    font-size: 13px;
  }
  .btn:hover:not(:disabled) {
    background: var(--bg-3);
    border-color: var(--fg-4);
  }
  .primary {
    background: var(--fg);
    color: var(--bg);
    border-color: var(--fg);
  }
  .primary:hover:not(:disabled) {
    background: var(--fg);
    opacity: 0.85;
    border-color: var(--fg);
  }
  .ghost {
    border-color: transparent;
    background: transparent;
    color: var(--fg-2);
  }
  .ghost:hover:not(:disabled) {
    background: var(--bg-3);
    border-color: transparent;
  }
  .danger {
    color: var(--red);
  }
  .danger:hover:not(:disabled) {
    background: var(--red-bg);
    border-color: var(--red);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .spin {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid currentColor;
    border-right-color: transparent;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
