<script lang="ts">
  import { api } from '../lib/api';
  import { parseAmount, toISO } from '../lib/format';
  import { errMsg, refresh, toast } from '../lib/state.svelte';
  import Alert from './Alert.svelte';
  import Button from './Button.svelte';

  let {
    accountId,
    currency,
    defaultDate,
    oncreated,
  }: { accountId: string; currency: string; defaultDate?: string; oncreated?: () => void } = $props();

  // The default only seeds the field; later prop changes must not overwrite what the user typed.
  // svelte-ignore state_referenced_locally
  let date = $state(defaultDate ?? toISO(new Date()));
  let amount = $state('');
  let note = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  const cents = $derived(parseAmount(amount));

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (cents === null) {
      error = 'Enter an amount like 1,234.56 (use a minus sign for money owed)';
      return;
    }
    busy = true;
    error = null;
    try {
      await api.setAnchor(accountId, date, cents, note || null);
      toast('Balance saved', 'success');
      amount = '';
      note = '';
      await refresh();
      oncreated?.();
    } catch (err) {
      error = errMsg(err);
    } finally {
      busy = false;
    }
  }
</script>

<form onsubmit={submit}>
  <div class="row">
    <div class="field">
      <label for="anc-date">Date</label>
      <input id="anc-date" class="input" type="date" bind:value={date} required />
    </div>
    <div class="field">
      <label for="anc-amt">Balance at end of that day ({currency})</label>
      <input id="anc-amt" class="input tnum" inputmode="decimal" placeholder="0.00" bind:value={amount} autocomplete="off" required />
    </div>
    <div class="field grow">
      <label for="anc-note">Note <span class="muted">(optional)</span></label>
      <input id="anc-note" class="input" placeholder="e.g. Opening balance, March statement" bind:value={note} />
    </div>
    <Button type="submit" variant="primary" loading={busy}>Save balance</Button>
  </div>
  <p class="hint">
    Transactions on that day are already included in the balance. Add several over time (for example from each statement) and
    Ledgerhold will check they agree with your transactions.
  </p>
  {#if error}<Alert tone="error">{error}</Alert>{/if}
</form>

<style>
  form {
    display: grid;
    gap: 10px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 12px;
  }
  .grow {
    flex: 1;
    min-width: 180px;
  }
  .row .field:nth-child(2) {
    min-width: 240px;
  }
</style>
