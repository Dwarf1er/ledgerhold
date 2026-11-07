<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '../lib/api';
  import type { AccountKind, Institution, Person } from '../lib/api';
  import { errMsg, refresh, toast } from '../lib/state.svelte';
  import Alert from './Alert.svelte';
  import Button from './Button.svelte';
  import Dialog from './Dialog.svelte';

  let {
    open = $bindable(false),
    institutions,
    people,
  }: { open?: boolean; institutions: Institution[]; people: Person[] } = $props();

  let name = $state('');
  let institutionId = $state('');
  let kind = $state<AccountKind>('checking');
  let currency = $state('CAD');
  let number = $state('');
  let owners = $state<string[]>([]);
  let balanceOnly = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Reset only when the dialog opens. A refresh that replaces `institutions`/`people`
  // while the user is typing (or mid-submit) must not wipe the form.
  $effect(() => {
    if (open) {
      untrack(() => {
        name = '';
        institutionId = institutions[0]?.id ?? '';
        kind = 'checking';
        currency = 'CAD';
        number = '';
        owners = [];
        balanceOnly = false;
        error = null;
      });
    }
  });

  const valid = $derived(name.trim().length > 0 && !!institutionId && /^[A-Za-z]{3}$/.test(currency.trim()));

  function toggle(id: string) {
    owners = owners.includes(id) ? owners.filter((x) => x !== id) : [...owners, id];
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!valid) return;
    busy = true;
    error = null;
    const label = name.trim();
    try {
      await api.addAccount({
        institutionId,
        name: label,
        kind,
        currency: currency.trim().toUpperCase(),
        number: number.trim() || null,
        ownerIds: owners,
        balanceOnly,
      });
      await refresh();
      toast(`Added ${label}`, 'success');
      open = false;
    } catch (err) {
      error = errMsg(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title="Add account" description="A chequing, savings or credit card account." width={500}>
  {#if institutions.length === 0}
    <Alert tone="info">Add a bank first, then create accounts for it.</Alert>
  {:else}
    <form id="acct-form" onsubmit={submit}>
      <div class="field">
        <label for="ac-name">Name</label>
        <input id="ac-name" class="input" placeholder="e.g. Joint chequing" bind:value={name} autocomplete="off" />
      </div>
      <div class="two">
        <div class="field">
          <label for="ac-bank">Bank</label>
          <select id="ac-bank" class="select" bind:value={institutionId}>
            {#each institutions as i (i.id)}<option value={i.id}>{i.name}</option>{/each}
          </select>
        </div>
        <div class="field">
          <label for="ac-kind">Type</label>
          <select id="ac-kind" class="select" bind:value={kind}>
            <option value="checking">Chequing</option>
            <option value="savings">Savings</option>
            <option value="credit_card">Credit card</option>
          </select>
        </div>
      </div>
      <div class="two">
        <div class="field">
          <label for="ac-cur">Currency</label>
          <input id="ac-cur" class="input mono" maxlength="3" bind:value={currency} />
        </div>
        <div class="field">
          <label for="ac-num">Account or card number <span class="muted">(optional)</span></label>
          <input id="ac-num" class="input mono" placeholder="As printed on statements" bind:value={number} autocomplete="off" />
        </div>
      </div>
      <p class="hint">
        The number lets Ledgerhold pick the right account when one file contains several (Desjardins CSVs do).
      </p>
      <fieldset class="field">
        <legend class="label">Owners</legend>
        {#if people.length === 0}
          <p class="hint">Add people in Settings first to assign owners.</p>
        {:else}
          <div class="owners">
            {#each people as p (p.id)}
              <label class="check"><input type="checkbox" checked={owners.includes(p.id)} onchange={() => toggle(p.id)} /> {p.name}</label>
            {/each}
          </div>
          <p class="hint">Choose more than one for a joint account.</p>
        {/if}
      </fieldset>
      <label class="check">
        <input type="checkbox" bind:checked={balanceOnly} /> Track by month-end balance only
      </label>
      <p class="hint">
        For accounts you value by hand, such as investments: enter a balance now and then. Ledgerhold will not
        check transactions against it.
      </p>
      {#if error}<Alert tone="error">{error}</Alert>{/if}
    </form>
  {/if}

  {#snippet footer()}
    <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
    {#if institutions.length}
      <Button type="submit" variant="primary" loading={busy} disabled={!valid} form="acct-form">Add account</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  form {
    display: grid;
    gap: 14px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
  }
  .owners {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
</style>
