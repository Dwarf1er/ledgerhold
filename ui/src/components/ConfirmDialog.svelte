<script lang="ts">
  import type { Snippet } from 'svelte';
  import Button from './Button.svelte';
  import Dialog from './Dialog.svelte';
  import Alert from './Alert.svelte';
  import { errMsg } from '../lib/state.svelte';

  let {
    open = $bindable(false),
    title,
    confirmLabel = 'Confirm',
    danger = false,
    onconfirm,
    children,
  }: {
    open?: boolean;
    title: string;
    confirmLabel?: string;
    danger?: boolean;
    onconfirm: () => Promise<void> | void;
    children: Snippet;
  } = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) error = null;
  });

  async function go() {
    busy = true;
    error = null;
    try {
      await onconfirm();
      open = false;
    } catch (e) {
      error = errMsg(e);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open {title} width={440}>
  <div class="msg">{@render children()}</div>
  {#if error}<div class="err"><Alert tone="error">{error}</Alert></div>{/if}
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
    <Button variant={danger ? 'danger' : 'primary'} loading={busy} onclick={go} data-autofocus>{confirmLabel}</Button>
  {/snippet}
</Dialog>

<style>
  .msg {
    color: var(--fg-2);
  }
  .err {
    margin-top: 12px;
  }
</style>
