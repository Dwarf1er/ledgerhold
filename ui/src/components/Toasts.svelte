<script lang="ts">
  import { dismissToast, toasts } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
</script>

<div class="stack" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === 'error' ? 'alert' : 'status'}>
      <span class="ico"><Icon name={t.kind === 'error' ? 'alert' : t.kind === 'success' ? 'check' : 'info'} size={15} /></span>
      <span class="msg">{t.message}</span>
      <button type="button" aria-label="Dismiss" onclick={() => dismissToast(t.id)}><Icon name="x" size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 100;
    display: grid;
    gap: 8px;
    width: min(380px, calc(100vw - 40px));
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 10px 10px 12px;
    background: var(--bg);
    border-radius: var(--radius-lg);
    box-shadow: var(--menu-shadow);
    font-size: 13px;
    animation: slide 0.16s ease-out;
  }
  .ico {
    display: flex;
    margin-top: 2px;
    color: var(--fg-3);
  }
  .success .ico {
    color: var(--green);
  }
  .error .ico {
    color: var(--red);
  }
  .msg {
    flex: 1;
    overflow-wrap: anywhere;
  }
  button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 0;
    background: none;
    border-radius: 4px;
    color: var(--fg-4);
  }
  button:hover {
    background: var(--bg-3);
    color: var(--fg);
  }
  @keyframes slide {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
</style>
