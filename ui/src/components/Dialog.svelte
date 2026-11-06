<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    open = $bindable(false),
    title,
    description,
    width = 480,
    onclose,
    children,
    footer,
  }: {
    open?: boolean;
    title: string;
    description?: string;
    width?: number;
    onclose?: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let panel = $state<HTMLDivElement>();
  let opener: Element | null = null;

  function close() {
    open = false;
    onclose?.();
  }

  const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
    } else if (e.key === 'Tab' && panel) {
      const items = [...panel.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (!items.length) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  $effect(() => {
    if (!open) return;
    opener = document.activeElement;
    const prev = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    queueMicrotask(() => {
      const target = panel?.querySelector<HTMLElement>('[data-autofocus]') ?? panel?.querySelector<HTMLElement>('input, select, textarea') ;
      (target ?? panel)?.focus();
    });
    return () => {
      document.body.style.overflow = prev;
      if (opener instanceof HTMLElement) opener.focus();
    };
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay" onmousedown={(e) => e.target === e.currentTarget && close()} {onkeydown}>
    <div
      class="panel"
      style="max-width:{width}px"
      role="dialog"
      aria-modal="true"
      aria-labelledby="dlg-title"
      tabindex="-1"
      bind:this={panel}
    >
      <header>
        <div>
          <h2 id="dlg-title">{title}</h2>
          {#if description}<p class="muted">{description}</p>{/if}
        </div>
        <button class="x" type="button" onclick={close} aria-label="Close"><Icon name="x" /></button>
      </header>
      <div class="body">{@render children()}</div>
      {#if footer}<footer>{@render footer()}</footer>{/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in srgb, var(--bg) 55%, transparent);
    backdrop-filter: blur(3px);
    animation: fade 0.12s ease-out;
  }
  .panel {
    width: 100%;
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border-radius: 12px;
    box-shadow: var(--menu-shadow);
    animation: pop 0.14s ease-out;
    outline: none;
  }
  header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding: 20px 20px 0;
  }
  header p {
    margin-top: 2px;
    font-size: 13px;
  }
  .x {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    background: transparent;
    border-radius: var(--radius);
    color: var(--fg-3);
    flex: none;
  }
  .x:hover {
    background: var(--bg-3);
    color: var(--fg);
  }
  .body {
    padding: 16px 20px 20px;
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
    background: var(--bg-2);
    border-radius: 0 0 12px 12px;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.985);
    }
  }
</style>
