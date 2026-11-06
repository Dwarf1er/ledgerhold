<script lang="ts">
  import { api } from '../lib/api';
  import type { AccountDetail, BalancePoint, Batch, Discrepancy, Txn } from '../lib/api';
  import Alert from '../components/Alert.svelte';
  import AnchorForm from '../components/AnchorForm.svelte';
  import Avatar from '../components/Avatar.svelte';
  import BalanceChart from '../components/BalanceChart.svelte';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ExportDialog from '../components/ExportDialog.svelte';
  import Icon from '../components/Icon.svelte';
  import Money from '../components/Money.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Segmented from '../components/Segmented.svelte';
  import Tabs from '../components/Tabs.svelte';
  import {
    formatDate,
    formatMoney,
    formatShortDate,
    formatSigned,
    kindLabel,
    maskNumber,
    parseISO,
    toISO,
  } from '../lib/format';
  import { links } from '../lib/router.svelte';
  import { app, errMsg, refresh, toast } from '../lib/state.svelte';

  let { id, tab }: { id: string; tab: 'transactions' | 'balance' | 'imports' } = $props();

  let detail = $state<AccountDetail | null>(null);
  let txns = $state<Txn[]>([]);
  let series = $state<BalancePoint[]>([]);
  let error = $state<string | null>(null);
  let loadingTxns = $state(true);

  // Filters
  let query = $state('');
  let search = $state('');
  let from = $state('');
  let to = $state('');
  let limit = $state(200);
  let range = $state<'30' | '90' | '365' | 'all'>('90');

  let exportOpen = $state(false);
  let undoBatch = $state<Batch | null>(null);
  let undoOpen = $state(false);
  let deleteAnchorId = $state<string | null>(null);
  let deleteOpen = $state(false);

  const summary = $derived(detail?.summary);
  const acct = $derived(summary?.account);
  const owners = $derived(
    (acct?.ownerIds ?? []).map((oid) => app.overview?.people.find((p) => p.id === oid)).filter((p) => !!p),
  );
  const cur = $derived(acct?.currency ?? 'CAD');
  const hasAnchor = $derived((detail?.anchors.length ?? 0) > 0);
  const filtered = $derived(!!(search || from || to));

  // Debounce the search box.
  $effect(() => {
    const q = query;
    const t = setTimeout(() => (search = q), 200);
    return () => clearTimeout(t);
  });

  async function loadDetail() {
    try {
      detail = await api.accountDetail(id);
      error = null;
    } catch (e) {
      error = errMsg(e);
    }
  }

  async function loadTxns() {
    loadingTxns = true;
    try {
      txns = await api.transactions(id, from || null, to || null, search || null);
      limit = 200;
    } catch (e) {
      error = errMsg(e);
    } finally {
      loadingTxns = false;
    }
  }

  async function loadSeries() {
    if (!detail || !detail.anchors.length) {
      series = [];
      return;
    }
    const end = toISO(new Date());
    let start: string;
    if (range === 'all') {
      start = [detail.summary.firstDate, detail.anchors[0].date].filter((d): d is string => !!d).sort()[0];
    } else {
      const d = new Date();
      d.setDate(d.getDate() - Number(range));
      start = toISO(d);
    }
    try {
      series = await api.balanceSeries(id, start, end);
    } catch (e) {
      series = [];
      error = errMsg(e);
    }
  }

  // Reload whenever the account, filters or the shared overview (after an import/anchor) change.
  $effect(() => {
    void app.overview;
    loadDetail();
  });
  $effect(() => {
    void [id, search, from, to, app.overview];
    loadTxns();
  });
  $effect(() => {
    void [detail, range, tab];
    if (tab === 'balance') loadSeries();
  });

  const inflow = $derived(txns.filter((t) => t.amount > 0).reduce((s, t) => s + t.amount, 0));
  const outflow = $derived(txns.filter((t) => t.amount < 0).reduce((s, t) => s + t.amount, 0));

  function explain(d: Discrepancy): string {
    const diff = formatMoney(Math.abs(d.difference), cur);
    return d.difference < 0
      ? `Your records are ${diff} higher than reported: a debit may be missing, or a credit was recorded twice.`
      : `Your records are ${diff} lower than reported: a credit may be missing, or a debit was recorded twice.`;
  }

  async function doUndo() {
    if (!undoBatch) return;
    const n = await api.undoBatch(undoBatch.id);
    toast(`Removed ${n} transaction${n === 1 ? '' : 's'}`, 'success');
    await refresh();
  }

  async function doDeleteAnchor() {
    if (!deleteAnchorId) return;
    await api.deleteAnchor(deleteAnchorId);
    toast('Balance removed', 'success');
    await refresh();
  }

  const tabItems = $derived([
    { id: 'transactions', label: 'Transactions', href: links.account(id) },
    { id: 'balance', label: 'Balance', href: links.account(id, 'balance') },
    { id: 'imports', label: `Imports${detail ? ` (${detail.batches.length})` : ''}`, href: links.account(id, 'imports') },
  ]);

  const firstDefaultAnchorDate = $derived.by(() => {
    const first = detail?.summary.firstDate;
    if (!first) return toISO(new Date());
    const d = parseISO(first);
    d.setDate(d.getDate() - 1);
    return toISO(d);
  });
</script>

{#if error && !detail}
  <Alert tone="error">{error}</Alert>
{:else if !detail || !acct || !summary}
  <div class="muted" role="status">Loading…</div>
{:else}
  <PageHeader title={acct.name} back={{ href: links.overview(), label: 'Overview' }}>
    {#snippet subtitle()}
      <span>{acct.institutionName} · {kindLabel(acct.kind)}{#if acct.number} · {maskNumber(acct.number)}{/if}</span>
      {#if owners.length}
        <span class="owners">{#each owners as p (p.id)}<Avatar name={p.name} size={20} />{/each}</span>
        <span>{owners.map((p) => p.name).join(' & ')}</span>
      {/if}
    {/snippet}
    {#snippet actions()}
      <Button onclick={() => (exportOpen = true)}><Icon name="download" size={14} /> Export OFX</Button>
      <Button variant="primary" href={links.import(acct.id)}><Icon name="upload" size={14} /> Import</Button>
    {/snippet}
  </PageHeader>

  <section class="stats">
    <div class="stat">
      <div class="label">Balance today</div>
      {#if summary.balance !== null}
        <div class="big mono"><Money minor={summary.balance} currency={cur} /></div>
      {:else}
        <div class="big none">—</div>
        <a class="link" href={links.account(id, 'balance')}>Set a starting balance</a>
      {/if}
    </div>
    <div class="stat">
      <div class="label">Transactions</div>
      <div class="big mono">{summary.txnCount}</div>
    </div>
    <div class="stat">
      <div class="label">History</div>
      <div class="med">
        {#if summary.firstDate}{formatDate(summary.firstDate)} <span class="muted">to</span> {formatDate(summary.lastDate!)}{:else}<span class="muted">Nothing imported yet</span>{/if}
      </div>
    </div>
  </section>

  <Tabs items={tabItems} value={tab} />

  <div class="pane">
    {#if tab === 'transactions'}
      <div class="toolbar">
        <label class="search">
          <Icon name="search" size={14} />
          <input class="input" type="search" placeholder="Search description or amount" bind:value={query} aria-label="Search transactions" />
        </label>
        <label class="date"><span class="sr-only">From</span><input class="input" type="date" bind:value={from} aria-label="From date" /></label>
        <span class="muted">to</span>
        <label class="date"><span class="sr-only">To</span><input class="input" type="date" bind:value={to} aria-label="To date" /></label>
        {#if filtered}
          <Button variant="ghost" size="sm" onclick={() => { query = ''; search = ''; from = ''; to = ''; }}>Clear</Button>
        {/if}
        <span class="totals muted">
          {#if txns.length}
            In <span class="mono pos">{formatSigned(inflow, cur)}</span> · Out <span class="mono">{formatSigned(outflow, cur)}</span>
          {/if}
        </span>
      </div>

      {#if summary.txnCount === 0}
        <EmptyState title="No transactions yet">
          Import a statement to see this account's activity.
          {#snippet action()}<Button variant="primary" href={links.import(acct.id)}>Import a statement</Button>{/snippet}
        </EmptyState>
      {:else if txns.length === 0 && !loadingTxns}
        <EmptyState title="No matches">Nothing matches these filters.</EmptyState>
      {:else}
        <div class="card table-wrap">
          <table>
            <thead>
              <tr>
                <th class="date-col">Date</th>
                <th>Description</th>
                <th class="num">Amount</th>
                {#if hasAnchor}<th class="num">Balance</th>{/if}
              </tr>
            </thead>
            <tbody>
              {#each txns.slice(0, limit) as t (t.id)}
                <tr>
                  <td class="date-col muted">{formatShortDate(t.date)}</td>
                  <td class="desc">{t.description}</td>
                  <td class="num"><Money minor={t.amount} currency={cur} tint /></td>
                  {#if hasAnchor}
                    <td class="num muted">{#if t.balance !== null}<Money minor={t.balance} currency={cur} />{/if}</td>
                  {/if}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if txns.length > limit}
          <div class="more">
            <Button size="sm" onclick={() => (limit += 200)}>Show more ({txns.length - limit} remaining)</Button>
          </div>
        {/if}
      {/if}
    {:else if tab === 'balance'}
      {#if !hasAnchor}
        <Alert tone="info">
          Tell Ledgerhold what this account's balance was on a particular day (for example from a statement). Balances for every other
          day are then calculated from your transactions.
        </Alert>
        <div class="gap"></div>
        <div class="card pad">
          <h2>Starting balance</h2>
          <div class="gap-sm"></div>
          <AnchorForm accountId={id} currency={cur} defaultDate={firstDefaultAnchorDate} />
        </div>
      {:else}
        <div class="chart-head">
          <h2>Balance over time</h2>
          <Segmented
            label="Range"
            size="sm"
            bind:value={range}
            items={[
              { id: '30', label: '30 days' },
              { id: '90', label: '90 days' },
              { id: '365', label: '1 year' },
              { id: 'all', label: 'All' },
            ]}
          />
        </div>
        <div class="card pad chart">
          <BalanceChart points={series} currency={cur} anchorDates={detail.anchors.map((a) => a.date)} />
        </div>

        <h2 class="sect">Reconciliation</h2>
        {#if detail.reconciliation.length === 0}
          <p class="muted">Add a second known balance (from a later statement) to check your transactions against it.</p>
        {:else}
          <div class="card">
            {#each detail.reconciliation as r (r.from + r.to)}
              <div class="rec" class:bad={!r.ok}>
                <div class="rec-top">
                  <span>{formatDate(r.from)} → {formatDate(r.to)}</span>
                  {#if r.ok}<Badge tone="green">Matches</Badge>{:else}<Badge tone="red">Off by {formatMoney(Math.abs(r.difference), cur)}</Badge>{/if}
                </div>
                <div class="rec-nums muted">
                  Expected <span class="mono">{formatMoney(r.expected, cur)}</span> · Reported <span class="mono">{formatMoney(r.actual, cur)}</span>
                </div>
                {#if !r.ok}<div class="rec-why">{explain(r)}</div>{/if}
              </div>
            {/each}
          </div>
        {/if}

        <h2 class="sect">Known balances</h2>
        <div class="card table-wrap">
          <table>
            <thead><tr><th class="date-col">Date</th><th class="num">Balance</th><th>Note</th><th></th></tr></thead>
            <tbody>
              {#each detail.anchors as a (a.id)}
                <tr>
                  <td class="date-col">{formatDate(a.date)}</td>
                  <td class="num"><Money minor={a.amount} currency={cur} /></td>
                  <td class="muted">{a.note ?? ''}</td>
                  <td class="act">
                    <Button variant="ghost" size="sm" aria-label="Remove balance for {a.date}" onclick={() => { deleteAnchorId = a.id; deleteOpen = true; }}>
                      <Icon name="trash" size={14} />
                    </Button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <div class="gap"></div>
        <div class="card pad">
          <h2>Add a known balance</h2>
          <div class="gap-sm"></div>
          <AnchorForm accountId={id} currency={cur} />
        </div>
      {/if}
    {:else}
      {#if detail.batches.length === 0}
        <EmptyState title="No imports yet">
          Statements you import appear here, and each import can be undone as a whole.
          {#snippet action()}<Button variant="primary" href={links.import(acct.id)}>Import a statement</Button>{/snippet}
        </EmptyState>
      {:else}
        <div class="card table-wrap">
          <table>
            <thead>
              <tr><th>File</th><th>Period</th><th class="num">New</th><th class="num">Duplicates</th><th class="num">Errors</th><th></th></tr>
            </thead>
            <tbody>
              {#each [...detail.batches].reverse() as b (b.id)}
                <tr>
                  <td>
                    <div>{b.filename}</div>
                    <div class="muted small">{b.parser} · {formatDate(b.importedAt.slice(0, 10))}</div>
                  </td>
                  <td class="muted">{#if b.periodStart}{formatShortDate(b.periodStart)} – {formatShortDate(b.periodEnd!)}{/if}</td>
                  <td class="num mono">{b.rowsNew}</td>
                  <td class="num mono muted">{b.rowsDuplicate}</td>
                  <td class="num mono">{#if b.rowsError}<Badge tone="amber">{b.rowsError}</Badge>{:else}<span class="muted">0</span>{/if}</td>
                  <td class="act">
                    <Button variant="ghost" size="sm" onclick={() => { undoBatch = b; undoOpen = true; }}><Icon name="undo" size={14} /> Undo</Button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}
  </div>

  <ExportDialog bind:open={exportOpen} accounts={app.overview?.accounts ?? []} selected={[id]} />

  <ConfirmDialog bind:open={undoOpen} title="Undo this import?" confirmLabel="Undo import" danger onconfirm={doUndo}>
    This removes the {undoBatch?.rowsNew ?? 0} transaction{undoBatch?.rowsNew === 1 ? '' : 's'} added from
    <strong>{undoBatch?.filename}</strong>. You can import the file again later.
  </ConfirmDialog>

  <ConfirmDialog bind:open={deleteOpen} title="Remove this known balance?" confirmLabel="Remove" danger onconfirm={doDeleteAnchor}>
    Balances for this account will be calculated from the remaining known balances.
  </ConfirmDialog>
{/if}

<style>
  .owners {
    display: inline-flex;
  }
  .owners :global(.avatar:not(:first-child)) {
    margin-left: -5px;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 32px;
    margin-bottom: 24px;
  }
  .label {
    font-size: 13px;
    color: var(--fg-3);
  }
  .big {
    font-size: 28px;
    line-height: 38px;
    font-weight: 600;
    letter-spacing: -0.03em;
  }
  .big.none {
    color: var(--fg-4);
  }
  .med {
    margin-top: 8px;
    font-size: 15px;
  }
  .link {
    font-size: 13px;
    color: var(--accent);
  }
  .pane {
    padding-top: 20px;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 220px;
    max-width: 340px;
  }
  .search :global(svg) {
    position: absolute;
    left: 10px;
    color: var(--fg-4);
    pointer-events: none;
  }
  .search .input {
    width: 100%;
    padding-left: 32px;
  }
  .totals {
    margin-left: auto;
    font-size: 13px;
  }
  .pos {
    color: var(--green);
  }
  .table-wrap {
    overflow: hidden;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    padding: 8px 16px;
    text-align: left;
    font-size: 12px;
    font-weight: 500;
    color: var(--fg-3);
    background: var(--bg-2);
    border-bottom: 1px solid var(--line);
  }
  td {
    padding: 0 16px;
    height: 40px;
    border-bottom: 1px solid var(--line);
  }
  tbody tr:last-child td {
    border-bottom: 0;
  }
  tbody tr:hover td {
    background: var(--bg-2);
  }
  .num {
    text-align: right;
    white-space: nowrap;
  }
  .date-col {
    width: 110px;
    white-space: nowrap;
  }
  .desc {
    max-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .act {
    width: 1%;
    text-align: right;
  }
  .small {
    font-size: 12px;
  }
  .more {
    display: flex;
    justify-content: center;
    margin-top: 16px;
  }
  .pad {
    padding: 16px 20px;
  }
  .chart {
    padding: 16px 12px 8px;
  }
  .chart-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }
  .sect {
    margin: 32px 0 12px;
  }
  .rec {
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
  }
  .rec:last-child {
    border-bottom: 0;
  }
  .rec-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .rec-nums {
    font-size: 13px;
    margin-top: 2px;
  }
  .rec-why {
    margin-top: 6px;
    font-size: 13px;
    color: var(--red);
  }
  .gap {
    height: 16px;
  }
  .gap-sm {
    height: 12px;
  }
</style>
