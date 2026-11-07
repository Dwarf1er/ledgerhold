<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import type { AccountSummary, Institution, Person } from '../lib/api';
  import AccountDialog from '../components/AccountDialog.svelte';
  import Alert from '../components/Alert.svelte';
  import Avatar from '../components/Avatar.svelte';
  import Button from '../components/Button.svelte';
  import ColorPicker from '../components/ColorPicker.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import Dialog from '../components/Dialog.svelte';
  import Icon from '../components/Icon.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import { nextFreeColor } from '../lib/colors';
  import { kindLabel, maskNumber } from '../lib/format';
  import { links } from '../lib/router.svelte';
  import { app, errMsg, refresh, session, toast } from '../lib/state.svelte';

  const ov = $derived(app.overview!);

  let personName = $state('');
  // Empty until the user picks one; until then the swatch shows (and the server would give) the first free colour.
  let personColor = $state('');
  const chosenColor = $derived(personColor || nextFreeColor(ov.people));
  let bankName = $state('');
  let bankId = $state('');
  let busy = $state<string | null>(null);
  let formError = $state<string | null>(null);
  let accountOpen = $state(false);
  let ownersOpen = $state(false);
  let ownersFor = $state<AccountSummary | null>(null);
  let ownerPick = $state<string[]>([]);
  let dbPath = $state('');

  // Rename (people and accounts share one dialog) and delete.
  let renameOpen = $state(false);
  let renameTarget = $state<{ kind: 'person' | 'account'; id: string; current: string; color?: string; number?: string; balanceOnly?: boolean } | null>(null);
  let renameBalanceOnly = $state(false);
  let renameNumber = $state('');
  let renameColor = $state('');
  let renameValue = $state('');
  let renameBusy = $state(false);
  let renameError = $state<string | null>(null);
  let bankEditOpen = $state(false);
  let bankEditing = $state<Institution | null>(null);
  let bankEditName = $state('');
  let bankEditId = $state('');
  let bankEditBusy = $state(false);
  let bankEditError = $state<string | null>(null);
  let deleteBankOpen = $state(false);
  let bankToDelete = $state<Institution | null>(null);
  let deletePersonOpen = $state(false);
  let personToDelete = $state<Person | null>(null);
  let deleteAccountOpen = $state(false);
  let accountToDelete = $state<AccountSummary | null>(null);

  let pwOpen = $state(false);
  let pwCurrent = $state('');
  let pwNew = $state('');
  let pwConfirm = $state('');
  let pwBusy = $state(false);
  let pwError = $state<string | null>(null);
  const pwReady = $derived(pwCurrent !== '' && pwNew.length >= 8 && pwNew === pwConfirm);

  function openPassword() {
    pwCurrent = pwNew = pwConfirm = '';
    pwError = null;
    pwOpen = true;
  }

  async function savePassword() {
    if (!pwReady) return;
    pwBusy = true;
    pwError = null;
    try {
      await api.changePassword(pwCurrent, pwNew);
      pwOpen = false;
      toast('Password changed. Anyone else signed in will need the new one.', 'success');
    } catch (err) {
      pwError = errMsg(err);
    } finally {
      pwBusy = false;
    }
  }

  onMount(() => {
    api.dbPath().then((p) => (dbPath = p)).catch(() => {});
  });

  async function addPerson(e: SubmitEvent) {
    e.preventDefault();
    if (!personName.trim()) return;
    busy = 'person';
    formError = null;
    try {
      await api.addPerson(personName, chosenColor);
      toast(`Added ${personName.trim()}`, 'success');
      personName = '';
      personColor = '';
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

  function startRename(kind: 'person' | 'account', id: string, current: string, color?: string, number?: string | null, balanceOnly = false) {
    renameTarget = { kind, id, current, color, number: number ?? '', balanceOnly };
    renameBalanceOnly = balanceOnly;
    renameNumber = number ?? '';
    renameValue = current;
    renameColor = color ?? '';
    renameError = null;
    renameOpen = true;
  }

  async function saveRename() {
    const t = renameTarget;
    const name = renameValue.trim();
    if (!t || !name) return;
    if (name === t.current && renameColor === (t.color ?? '') && renameNumber.trim() === (t.number ?? '') && renameBalanceOnly === !!t.balanceOnly) {
      renameOpen = false;
      return;
    }
    renameBusy = true;
    renameError = null;
    try {
      if (t.kind === 'person') await api.updatePerson(t.id, name, renameColor || null);
      else {
        await api.updateAccount(t.id, name, renameNumber.trim() || null);
        if (renameBalanceOnly !== !!t.balanceOnly) await api.setBalanceOnly(t.id, renameBalanceOnly);
      }
      await refresh();
      toast(t.kind === 'person' ? `Saved ${name}` : `Saved ${name}`, 'success');
      renameOpen = false;
    } catch (err) {
      renameError = errMsg(err);
    } finally {
      renameBusy = false;
    }
  }

  function startEditBank(i: Institution) {
    bankEditing = i;
    bankEditName = i.name;
    bankEditId = i.bankId ?? '';
    bankEditError = null;
    bankEditOpen = true;
  }

  async function saveBank() {
    if (!bankEditing || !bankEditName.trim()) return;
    bankEditBusy = true;
    bankEditError = null;
    try {
      await api.updateInstitution(bankEditing.id, bankEditName, bankEditId || null);
      await refresh();
      toast(`Saved ${bankEditName.trim()}`, 'success');
      bankEditOpen = false;
    } catch (err) {
      bankEditError = errMsg(err);
    } finally {
      bankEditBusy = false;
    }
  }

  async function doDeleteBank() {
    if (!bankToDelete) return;
    const { id, name } = bankToDelete;
    await api.deleteInstitution(id);
    await refresh();
    toast(`Deleted ${name}`, 'success');
  }

  const accountsAt = (id: string) => ov.accounts.filter((s) => s.account.institutionId === id).length;

  async function doDeletePerson() {
    if (!personToDelete) return;
    const { id, name } = personToDelete;
    await api.deletePerson(id);
    if (app.person === id) app.person = 'all';
    await refresh();
    toast(`Removed ${name}`, 'success');
  }

  async function doDeleteAccount() {
    if (!accountToDelete) return;
    const { name } = accountToDelete.account;
    await api.deleteAccount(accountToDelete.account.id);
    await refresh();
    toast(`Deleted ${name}`, 'success');
  }

  const ownedBy = (id: string) => ov.accounts.filter((s) => s.account.ownerIds.includes(id)).length;

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
      <div class="line">
        <Avatar name={p.name} color={p.color} size={24} /> <span>{p.name}</span>
        <span class="push actions">
          <Button variant="ghost" size="sm" aria-label={`Edit ${p.name}`} onclick={() => startRename('person', p.id, p.name, p.color)}><Icon name="pencil" size={14} /></Button>
          <Button variant="ghost" size="sm" aria-label={`Remove ${p.name}`} onclick={() => { personToDelete = p; deletePersonOpen = true; }}><Icon name="trash" size={14} /></Button>
        </span>
      </div>
    {:else}
      <div class="line muted">No one yet.</div>
    {/each}
    <form class="add" onsubmit={addPerson}>
      <input class="input" placeholder="Add a person" aria-label="Person name" bind:value={personName} autocomplete="off" />
      <ColorPicker label="Avatar colour" size={20} value={chosenColor} onchange={(c) => (personColor = c)} />
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
        <span class="actions" class:push={!i.bankId}>
          <Button variant="ghost" size="sm" aria-label={`Edit ${i.name}`} onclick={() => startEditBank(i)}><Icon name="pencil" size={14} /></Button>
          <Button variant="ghost" size="sm" aria-label={`Delete ${i.name}`} onclick={() => { bankToDelete = i; deleteBankOpen = true; }}><Icon name="trash" size={14} /></Button>
        </span>
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
        <span class="muted">{[s.account.institutionName, kindLabel(s.account.kind), s.account.currency, maskNumber(s.account.number)].filter(Boolean).join(' · ')}</span>
        <span class="push owners">
          {#each ownersOf(s.account.ownerIds) as p (p.id)}<Avatar name={p.name} color={p.color} size={20} />{/each}
          <Button variant="ghost" size="sm" onclick={() => editOwners(s)}>Owners</Button>
          <Button variant="ghost" size="sm" aria-label={`Edit ${s.account.name}`} onclick={() => startRename('account', s.account.id, s.account.name, undefined, s.account.number, s.account.balanceOnly)}><Icon name="pencil" size={14} /></Button>
          <Button variant="ghost" size="sm" aria-label={`Delete ${s.account.name}`} onclick={() => { accountToDelete = s; deleteAccountOpen = true; }}><Icon name="trash" size={14} /></Button>
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

{#if session.canChangePassword}
  <section>
    <div class="head">
      <div>
        <h2>Security</h2>
        <p class="muted">One password is shared by everyone in the household.</p>
      </div>
    </div>
    <div class="card">
      <div class="line">
        <span>Household password</span>
        <span class="push"><Button size="sm" onclick={openPassword}>Change password</Button></span>
      </div>
    </div>
  </section>
{/if}

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
        <Avatar name={p.name} color={p.color} size={20} /> {p.name}
      </label>
    {/each}
  </div>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (ownersOpen = false)}>Cancel</Button>
    <Button variant="primary" onclick={saveOwners}>Save</Button>
  {/snippet}
</Dialog>

<Dialog bind:open={renameOpen} title={renameTarget?.kind === 'account' ? 'Edit account' : 'Edit person'} description={renameTarget?.current} width={400}>
  <form onsubmit={(e) => { e.preventDefault(); saveRename(); }}>
    <div class="field">
      <label for="rename-name">Name</label>
      <input id="rename-name" class="input" bind:value={renameValue} autocomplete="off" data-autofocus />
    </div>
    {#if renameTarget?.kind === 'account'}
      <div class="field">
        <label for="rename-number">Account or card number <span class="muted">(optional)</span></label>
        <input id="rename-number" class="input mono" placeholder="As printed on statements" bind:value={renameNumber} autocomplete="off" />
      </div>
      <label class="check">
        <input type="checkbox" bind:checked={renameBalanceOnly} /> Track by month-end balance only
      </label>
      <p class="hint">For accounts you value by hand, such as investments: no checking against transactions.</p>
    {/if}
    {#if renameTarget?.kind === 'person'}
      <div class="field colour">
        <span class="label">Avatar colour</span>
        <ColorPicker label="Avatar colour" bind:value={renameColor} />
      </div>
    {/if}
    {#if renameError}<div class="gap"></div><Alert tone="error">{renameError}</Alert>{/if}
    <button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (renameOpen = false)}>Cancel</Button>
    <Button variant="primary" loading={renameBusy} disabled={!renameValue.trim()} onclick={saveRename}>Save</Button>
  {/snippet}
</Dialog>

<Dialog bind:open={bankEditOpen} title="Edit bank" description={bankEditing?.name} width={400}>
  <form class="pwform" onsubmit={(e) => { e.preventDefault(); saveBank(); }}>
    <div class="field">
      <label for="bank-name">Bank name</label>
      <input id="bank-name" class="input" bind:value={bankEditName} autocomplete="off" data-autofocus />
    </div>
    <div class="field">
      <label for="bank-ofx-id">OFX bank ID (optional)</label>
      <input id="bank-ofx-id" class="input mono" bind:value={bankEditId} autocomplete="off" />
    </div>
    {#if bankEditError}<Alert tone="error">{bankEditError}</Alert>{/if}
    <button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (bankEditOpen = false)}>Cancel</Button>
    <Button variant="primary" loading={bankEditBusy} disabled={!bankEditName.trim()} onclick={saveBank}>Save</Button>
  {/snippet}
</Dialog>

<ConfirmDialog bind:open={deleteBankOpen} title="Delete this bank?" confirmLabel="Delete bank" danger onconfirm={doDeleteBank}>
  {#if bankToDelete}
    {#if accountsAt(bankToDelete.id)}
      <strong>{bankToDelete.name}</strong> still has {accountsAt(bankToDelete.id)} account{accountsAt(bankToDelete.id) === 1 ? '' : 's'}.
      Delete {accountsAt(bankToDelete.id) === 1 ? 'it' : 'them'} first and then the bank can be removed.
    {:else}
      <strong>{bankToDelete.name}</strong> will be removed.
    {/if}
  {/if}
</ConfirmDialog>

<ConfirmDialog bind:open={deletePersonOpen} title="Remove this person?" confirmLabel="Remove" danger onconfirm={doDeletePerson}>
  {#if personToDelete}
    <strong>{personToDelete.name}</strong> will be removed from the household.
    {#if ownedBy(personToDelete.id)}
      Their {ownedBy(personToDelete.id)} account{ownedBy(personToDelete.id) === 1 ? '' : 's'} will stay, without them as an owner.
    {/if}
  {/if}
</ConfirmDialog>

<ConfirmDialog bind:open={deleteAccountOpen} title="Delete this account?" confirmLabel="Delete account" danger onconfirm={doDeleteAccount}>
  {#if accountToDelete}
    This permanently deletes <strong>{accountToDelete.account.name}</strong> with its
    {accountToDelete.txnCount} transaction{accountToDelete.txnCount === 1 ? '' : 's'}, imports and known balances. This can't be undone.
  {/if}
</ConfirmDialog>

<Dialog bind:open={pwOpen} title="Change password" description="Everyone else will be signed out." width={400}>
  <form class="pwform" onsubmit={(e) => { e.preventDefault(); savePassword(); }}>
    <div class="field">
      <label for="pw-current">Current password</label>
      <input id="pw-current" class="input" type="password" autocomplete="current-password" bind:value={pwCurrent} data-autofocus />
    </div>
    <div class="field">
      <label for="pw-new">New password</label>
      <input id="pw-new" class="input" type="password" autocomplete="new-password" bind:value={pwNew} />
    </div>
    <div class="field">
      <label for="pw-confirm">Confirm new password</label>
      <input id="pw-confirm" class="input" type="password" autocomplete="new-password" bind:value={pwConfirm} />
    </div>
    {#if pwNew && pwNew.length < 8}<p class="muted small">Use at least 8 characters.</p>
    {:else if pwConfirm && pwConfirm !== pwNew}<p class="muted small">The two passwords do not match.</p>{/if}
    {#if pwError}<Alert tone="error">{pwError}</Alert>{/if}
    <button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (pwOpen = false)}>Cancel</Button>
    <Button variant="primary" loading={pwBusy} disabled={!pwReady} onclick={savePassword}>Change password</Button>
  {/snippet}
</Dialog>

<style>
  .pwform {
    display: grid;
    gap: 12px;
  }
  .colour {
    margin-top: 12px;
  }
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
  .owners,
  .actions {
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
