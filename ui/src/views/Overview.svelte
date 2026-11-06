<script lang="ts">
  import Avatar from '../components/Avatar.svelte';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import Icon from '../components/Icon.svelte';
  import Money from '../components/Money.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Segmented from '../components/Segmented.svelte';
  import { formatMoney, formatShortDate, kindLabel, maskNumber } from '../lib/format';
  import { links } from '../lib/router.svelte';
  import { app } from '../lib/state.svelte';

  const ov = $derived(app.overview!);
  const people = $derived(ov.people);
  const rows = $derived(
    [...ov.accounts]
      .filter((s) => app.person === 'all' || s.account.ownerIds.includes(app.person))
      .sort((a, b) => a.account.name.localeCompare(b.account.name)),
  );

  const totals = $derived.by(() => {
    const map = new Map<string, { net: number; assets: number; owed: number }>();
    for (const s of rows) {
      if (s.balance === null) continue;
      const t = map.get(s.account.currency) ?? { net: 0, assets: 0, owed: 0 };
      t.net += s.balance;
      if (s.balance >= 0) t.assets += s.balance;
      else t.owed += s.balance;
      map.set(s.account.currency, t);
    }
    return [...map.entries()].map(([currency, t]) => ({ currency, ...t }));
  });

  const unanchored = $derived(rows.filter((s) => s.balance === null));
  const hasJoint = $derived(app.person !== 'all' && rows.some((s) => s.account.ownerIds.length > 1));
  const filterItems = $derived([{ id: 'all', label: 'Everyone' }, ...people.map((p) => ({ id: p.id, label: p.name }))]);
  const details = (a: { institutionName: string; kind: string; number: string | null }) =>
    [a.institutionName, kindLabel(a.kind), maskNumber(a.number)].filter(Boolean).join(' · ');
  const ownersOf = (ids: string[]) => ids.map((id) => people.find((p) => p.id === id)).filter((p) => !!p);
</script>

<PageHeader title="Overview">
  {#snippet subtitle()}
    {#if rows.length}{rows.length} account{rows.length === 1 ? '' : 's'}{#if app.person !== 'all'} · {people.find((p) => p.id === app.person)?.name}{/if}{/if}
  {/snippet}
  {#snippet actions()}
    {#if people.length > 1}
      <Segmented label="Show accounts for" items={filterItems} bind:value={app.person} />
    {/if}
    {#if ov.accounts.length}
      <Button variant="primary" href={links.import()}><Icon name="upload" size={14} /> Import</Button>
    {/if}
  {/snippet}
</PageHeader>

{#if ov.accounts.length === 0}
  <EmptyState title="Set up your household">
    Add the people who share the accounts, then a bank and its accounts. After that, import statements and Ledgerhold keeps the
    balances, removes duplicates and exports OFX for any date range.
    {#snippet action()}
      <Button variant="primary" href={links.settings()}>Get started</Button>
    {/snippet}
  </EmptyState>
{:else}
  <section class="cards" aria-label="Totals">
    {#each totals as t (t.currency)}
      <div class="card stat">
        <div class="label">Net balance · {t.currency}</div>
        <div class="big tnum">{formatMoney(t.net, t.currency)}</div>
        <div class="sub muted">
          <span>Assets <span class="tnum">{formatMoney(t.assets, t.currency)}</span></span>
          {#if t.owed < 0}<span>Owed <span class="tnum">{formatMoney(Math.abs(t.owed), t.currency)}</span></span>{/if}
        </div>
      </div>
    {/each}
    {#if unanchored.length}
      <div class="card stat attention">
        <div class="label">Needs a starting balance</div>
        <div class="big">{unanchored.length} account{unanchored.length === 1 ? '' : 's'}</div>
        <div class="sub muted">Balances appear once an account has a known balance on a date.</div>
      </div>
    {/if}
  </section>
  {#if hasJoint}
    <p class="note muted">Joint accounts are counted in full for each owner.</p>
  {/if}

  <h2 class="section">Accounts</h2>
  <div class="card list">
    {#each rows as s (s.account.id)}
      <a class="row" href={links.account(s.account.id)}>
        <span class="icon"><Icon name={s.account.kind === 'credit_card' ? 'card' : 'bank'} size={16} /></span>
        <span class="main">
          <span class="name">{s.account.name}</span>
          <span class="meta muted">
            {details(s.account)}
          </span>
        </span>
        <span class="owners" aria-label="Owners">
          {#each ownersOf(s.account.ownerIds) as p (p.id)}<Avatar name={p.name} />{/each}
        </span>
        <span class="activity muted">
          {#if s.txnCount}
            {s.txnCount} transactions<br />
            <span class="small">last {formatShortDate(s.lastDate!)}</span>
          {:else}
            No transactions yet
          {/if}
        </span>
        <span class="bal">
          {#if s.balance !== null}
            <strong class="tnum"><Money minor={s.balance} currency={s.account.currency} /></strong>
          {:else}
            <Badge tone="amber">Set starting balance</Badge>
          {/if}
        </span>
        <span class="chev"><Icon name="chevron-right" size={16} /></span>
      </a>
    {:else}
      <div class="none muted">No accounts for this person yet.</div>
    {/each}
  </div>
{/if}

<style>
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 16px;
  }
  .stat {
    padding: 16px 20px;
  }
  .label {
    font-size: 13px;
    color: var(--fg-3);
  }
  .big {
    margin: 4px 0;
    font-size: 28px;
    line-height: 36px;
    font-weight: 600;
    letter-spacing: -0.03em;
  }
  .sub {
    display: flex;
    gap: 16px;
    font-size: 13px;
  }
  .attention {
    background: var(--amber-bg);
    border-color: transparent;
  }
  .attention .label,
  .attention .sub {
    color: var(--amber);
  }
  .note {
    margin-top: 10px;
    font-size: 12px;
  }
  .section {
    margin: 32px 0 12px;
  }
  .list {
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: 28px minmax(0, 1.6fr) auto minmax(120px, 1fr) minmax(130px, auto) 16px;
    align-items: center;
    gap: 16px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--line);
    transition: background 0.1s;
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row:hover {
    background: var(--bg-2);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    background: var(--bg-3);
    color: var(--fg-2);
  }
  .main {
    display: grid;
    min-width: 0;
  }
  .name {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .owners {
    display: flex;
  }
  .owners :global(.avatar:not(:first-child)) {
    margin-left: -6px;
  }
  .activity {
    font-size: 13px;
    line-height: 1.35;
  }
  .small {
    font-size: 12px;
  }
  .bal {
    text-align: right;
    font-size: 15px;
  }
  .chev {
    color: var(--fg-4);
  }
  .none {
    padding: 24px;
    text-align: center;
  }
  @media (max-width: 900px) {
    .row {
      grid-template-columns: 28px minmax(0, 1fr) auto;
    }
    .owners,
    .activity,
    .chev {
      display: none;
    }
  }
</style>
