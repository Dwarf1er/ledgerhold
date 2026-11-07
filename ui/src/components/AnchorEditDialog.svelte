<script lang="ts">
  import { api } from '../lib/api';
  import type { Anchor } from '../lib/api';
  import { formatDate, parseAmount, plainAmount } from '../lib/format';
  import { errMsg, refresh, toast } from '../lib/state.svelte';
  import Alert from './Alert.svelte';
  import Button from './Button.svelte';
  import Dialog from './Dialog.svelte';

  let {
    open = $bindable(false),
    accountId,
    currency,
    anchor,
    all,
  }: { open?: boolean; accountId: string; currency: string; anchor: Anchor | null; all: Anchor[] } = $props();

  let date = $state('');
  let amount = $state('');
  let note = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Fill the fields each time the dialog opens, not when the overview refreshes underneath it.
  $effect(() => {
    if (open && anchor) {
      date = anchor.date;
      amount = plainAmount(anchor.amount);
      note = anchor.note ?? '';
      error = null;
    }
  });

  async function save() {
    if (!anchor) return;
    const cents = parseAmount(amount);
    if (cents === null) {
      error = 'Enter an amount like 1,234.56 (use a minus sign for money owed)';
      return;
    }
    if (!date) {
      error = 'Pick a date';
      return;
    }
    const clash = all.find((a) => a.date === date && a.id !== anchor.id);
    if (clash) {
      error = `There is already a known balance for ${formatDate(date)}. Edit or remove that one instead.`;
      return;
    }
    busy = true;
    error = null;
    try {
      // Save the new one first so a failure never leaves the account without a balance.
      await api.setAnchor(accountId, date, cents, note.trim() || null);
      if (date !== anchor.date) await api.deleteAnchor(anchor.id);
      await refresh();
      toast('Balance updated', 'success');
      open = false;
    } catch (err) {
      error = errMsg(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title="Edit known balance" description={anchor ? formatDate(anchor.date) : undefined} width={440}>
  <form class="form" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <div class="field">
      <label for="anc-edit-date">Date</label>
      <input id="anc-edit-date" class="input" type="date" bind:value={date} required />
    </div>
    <div class="field">
      <label for="anc-edit-amt">Balance at end of that day ({currency})</label>
      <input id="anc-edit-amt" class="input tnum" inputmode="decimal" bind:value={amount} autocomplete="off" required data-autofocus />
    </div>
    <div class="field">
      <label for="anc-edit-note">Note <span class="muted">(optional)</span></label>
      <input id="anc-edit-note" class="input" bind:value={note} />
    </div>
    {#if error}<Alert tone="error">{error}</Alert>{/if}
    <button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
    <Button variant="primary" loading={busy} onclick={save}>Save</Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: grid;
    gap: 12px;
  }
</style>
