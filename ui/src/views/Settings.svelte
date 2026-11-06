<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import type { AccountSummary } from '../lib/api';
  import AccountDialog from '../components/AccountDialog.svelte';
  import Alert from '../components/Alert.svelte';
  import Avatar from '../components/Avatar.svelte';
  import Button from '../components/Button.svelte';
  import Dialog from '../components/Dialog.svelte';
  import Icon from '../components/Icon.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import { kindLabel, maskNumber } from '../lib/format';
  import { links } from '../lib/router.svelte';
  import { app, errMsg, refresh, toast } from '../lib/state.svelte';

  const ov = $derived(app.overview!);

  let personName = $state('');
  let bankName = $state('');
  let bankId = $state('');
  let busy = $state<string | null>(null);
  let formError = $state<string | null>(null);
  let accountOpen = $state(false);
  let ownersOpen = $state(false);
  let ownersFor = $state<AccountSummary | null>(null);
  let ownerPick = $state<string[]>([]);
  let dbPath = $state('');

  onMount(() => {
    api.dbPath().then((p) => (dbPath = p)).catch(() => {});
  });

  async function addPerson(e: SubmitEvent) {
    e.preventDefault();
    if (!personName.trim()) return;
    busy = 'person';
    formError = null;
    try {
      await api.addPerson(personName);
      toast(`Added ${personName.trim()}`, 'success');
      personName = '';
      await refresh();
    } catch (err) {
      formError = errMsg(err);
    } finally {
      busy = null;
    }
  }

  async function addBank(e: SubmitEvent) {
    e.preventDefault();
    if (!bankName.trim()) return;
    busy = 'bank';
    formError = null;
    try {
      await api.addInstitution(bankName, bankId || null);
      toast(`Added ${bankName.trim()}`, 'success');
      bankName = '';
      bankId = '';
      await refresh();
    } catch (err) {
      formError = errMsg(err);
    } finally {
      busy = null;
    }
  }

  function editOwners(s: AccountSummary) {
    ownersFor = s;
    ownerPick = [...s.account.ownerIds];
    ownersOpen = true;
  }

  async function saveOwners() {
    if (!ownersFor) return;
    try {
      await api.setOwners(ownersFor.account.id, ownerPick);
      await refresh();
      toast('Owners updated', 'success');
      ownersOpen = false;
    } catch (err) {
      toast(errMsg(err), 'error');
    }
  }

  const ownersOf = (ids: string[]) => ids.map((id) => ov.people.find((p) => p.id === id)).filter((p) => !!p);
</script>

<PageHeader title="Settings" />

{#if formError}<Alert tone="error">{formError}</Alert><div class="gap"></div>{/if}

<section>
  <div class="head">
    <div>
      <h2>People</h2>
      <p class="muted">Household members. Accounts can belong to one person or be shared.</p>
    </div>
  </div>
  <div class="card">
    {#each ov.people as p (p.id)}
      <div class="line"><Avatar name={p.name} size={24} /> <span>{p.name}</span></div>
    {:else}
      <div class="line muted">No one yet.</div>
    {/each}
    <form class="add" onsubmit={addPerson}>
      <input class="input" placeholder="Add a person" aria-label="Person name" bind:value={personName} autocomplete="off" />
      <Button type="submit" size="sm" loading={busy === 'person'} disabled={!personName.trim()}><Icon name="plus" size={14} /> Add</Button>
    </form>
  </div>
</section>

<section>
  <div class="head">
    <div>
      <h2>Banks</h2>
      <p class="muted">The OFX bank ID is written into exports; your budgeting app may ask for it.</p>
    </div>
  </div>
  <div class="card">
    {#each ov.institutions as i (i.id)}
      <div class="line">
        <span class="ico"><Icon name="bank" size={14} /></span>
        <span>{i.name}</span>
        {#if i.bankId}<span class="muted mono push">ID {i.bankId}</span>{/if}
      </div>
    {:else}
      <div class="line muted">No banks yet.</div>
    {/each}
    <form class="add" onsubmit={addBank}>
      <input class="input" placeholder="Bank name" aria-label="Bank name" bind:value={bankName} autocomplete="off" />
      <input class="input short mono" placeholder="OFX bank ID (optional)" aria-label="OFX bank ID" bind:value={bankId} autocomplete="off" />
      <Button type="submit" size="sm" loading={busy === 'bank'} disabled={!bankName.trim()}><Icon name="plus" size={14} /> Add</Button>
    </form>
  </div>
</section>

<section>
  <div class="head">
    <div>
      <h2>Accounts</h2>
      <p class="muted">Chequing, savings and credit cards.</p>
    </div>
    <Button variant="primary" size="sm" onclick={() => (accountOpen = true)}><Icon name="plus" size={14} /> Add account</Button>
  </div>
  <div class="card">
    {#each ov.accounts as s (s.account.id)}
      <div class="line acct">
        <span class="ico"><Icon name={s.account.kind === 'credit_card' ? 'card' : 'bank'} size={14} /></span>
        <a class="nm" href={links.account(s.account.id)}>{s.account.name}</a>
        <span class="muted">{s.account.institutionName} · {kindLabel(s.account.kind)} · {s.account.currency}{#if s.account.number} · {maskNumber(s.account.number)}{/if}</span>
        <span class="push owners">
          {#each ownersOf(s.account.ownerIds) as p (p.id)}<Avatar name={p.name} size={20} />{/each}
          <Button variant="ghost" size="sm" onclick={() => editOwners(s)}>Owners</Button>
        </span>
      </div>
    {:else}
      <div class="line muted">No accounts yet.</div>
    {/each}
  </div>
</section>

<section>
  <div class="head">
    <div>
      <h2>Data</h2>
      <p class="muted">Everything is stored in one local SQLite file. The command-line tool uses the same file.</p>
    </div>
  </div>
  <div class="card"><div class="line"><span class="mono small">{dbPath || '…'}</span></div></div>
</section>

<AccountDialog bind:open={accountOpen} institutions={ov.institutions} people={ov.people} />

<Dialog bind:open={ownersOpen} title="Account owners" description={ownersFor?.account.name} width={400}>
  <div class="pick">
    {#each ov.people as p (p.id)}
      <label class="check">
        <input
          type="checkbox"
          checked={ownerPick.includes(p.id)}
          onchange={() => (ownerPick = ownerPick.includes(p.id) ? ownerPick.filter((x) => x !== p.id) : [...ownerPick, p.id])}
        />
        <Avatar name={p.name} size={20} /> {p.name}
      </label>
    {/each}
  </div>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (ownersOpen = false)}>Cancel</Button>
    <Button variant="primary" onclick={saveOwners}>Save</Button>
  {/snippet}
</Dialog>

<style>
  section {
    margin-bottom: 36px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: end;
    gap: 16px;
    margin-bottom: 12px;
  }
  .head p {
    font-size: 13px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    min-height: 44px;
    border-bottom: 1px solid var(--line);
  }
  .push {
    margin-left: auto;
  }
  .ico {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: var(--bg-3);
    color: var(--fg-2);
    flex: none;
  }
  .nm {
    font-weight: 500;
  }
  .acct .muted {
    font-size: 13px;
  }
  .owners {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .add {
    display: flex;
    gap: 8px;
    padding: 10px 16px;
    background: var(--bg-2);
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
  }
  .add .input {
    flex: 1;
    min-width: 0;
  }
  .add .short {
    max-width: 220px;
  }
  .small {
    font-size: 12px;
  }
  .gap {
    height: 16px;
  }
  .pick {
    display: grid;
    gap: 10px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 10px;
  }
</style>
