<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '../lib/api';
  import type { AccountSummary, ExportResult } from '../lib/api';
  import { formatMonth, formatMoney, lastFullMonth, monthRange, parseISO, shiftMonth, toISO } from '../lib/format';
  import { errMsg, lastExportDir, rememberExportDir, toast } from '../lib/state.svelte';
  import Alert from './Alert.svelte';
  import Button from './Button.svelte';
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import Segmented from './Segmented.svelte';

  let {
    open = $bindable(false),
    accounts,
    selected = [],
  }: { open?: boolean; accounts: AccountSummary[]; selected?: string[] } = $props();

  type Mode = 'month' | 'custom' | 'all';
  let mode = $state<Mode>('month');
  let picked = $state<string[]>([]);
  let ym = $state(lastFullMonth(new Date()));
  let from = $state(toISO(new Date()));
  let to = $state(toISO(new Date()));
  let dir = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let results = $state<ExportResult[] | null>(null);

  // Reset each time the dialog opens (and only then: a background refresh of `accounts`
  // must not undo what the user has chosen).
  $effect(() => {
    if (open) {
      untrack(() => {
        picked = selected.length ? [...selected] : accounts.length === 1 ? [accounts[0].account.id] : [];
        ym = lastFullMonth(new Date());
        mode = 'month';
        dir = lastExportDir();
        error = null;
        results = null;
      });
    }
  });

  const chosen = $derived(accounts.filter((s) => picked.includes(s.account.id)));
  const range = $derived(mode === 'month' ? monthRange(ym.year, ym.month) : { start: from, end: to });
  const span = $derived.by(() => {
    const firsts = chosen.map((s) => s.firstDate).filter((d): d is string => !!d);
    const lasts = chosen.map((s) => s.lastDate).filter((d): d is string => !!d);
    return firsts.length ? { first: firsts.sort()[0], last: lasts.sort().reverse()[0] } : null;
  });
  const months = $derived.by(() => {
    if (!span) return [];
    const out: { start: string; end: string }[] = [];
    const a = parseISO(span.first);
    const b = parseISO(span.last);
    let y = a.getFullYear();
    let m = a.getMonth() + 1;
    while (y < b.getFullYear() || (y === b.getFullYear() && m <= b.getMonth() + 1)) {
      out.push(monthRange(y, m));
      ({ year: y, month: m } = shiftMonth(y, m, 1));
    }
    return out;
  });

  const invalid = $derived(
    chosen.length === 0
      ? 'Choose at least one account'
      : !dir
        ? 'Choose a destination folder'
        : mode === 'custom' && (!from || !to || to < from)
          ? 'The end date must be on or after the start date'
          : mode === 'all' && months.length === 0
            ? 'The selected accounts have no transactions'
            : null,
  );

  function toggle(id: string) {
    picked = picked.includes(id) ? picked.filter((x) => x !== id) : [...picked, id];
  }

  async function chooseFolder() {
    const d = await api.pickFolder();
    if (d) dir = d;
  }

  async function run() {
    if (invalid || !dir) return;
    busy = true;
    error = null;
    try {
      const ids = chosen.map((s) => s.account.id);
      const out: ExportResult[] = [];
      if (mode === 'all') {
        for (const m of months) out.push(...(await api.exportOfx(ids, m.start, m.end, dir)));
      } else {
        out.push(...(await api.exportOfx(ids, range.start, range.end, dir)));
      }
      rememberExportDir(dir);
      results = out;
      toast(`Exported ${out.length} file${out.length === 1 ? '' : 's'}`, 'success');
    } catch (e) {
      error = errMsg(e);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title="Export OFX" description="One file per account, ready for your budgeting app." width={540}>
  {#if results}
    <div class="results">
      <Alert tone="success">Wrote {results.length} file{results.length === 1 ? '' : 's'} to <span class="mono">{dir}</span></Alert>
      <ul>
        {#each results as r (r.path)}
          <li>
            <span class="file mono">{r.path.split('/').pop()}</span>
            <span class="muted">{r.txnCount} transaction{r.txnCount === 1 ? '' : 's'}{#if r.balance !== null} · ends at {formatMoney(r.balance)}{/if}</span>
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <div class="form">
      {#if accounts.length > 1}
        <fieldset class="field">
          <legend class="label">Accounts</legend>
          <div class="accts">
            {#each accounts as s (s.account.id)}
              <label class="check">
                <input type="checkbox" checked={picked.includes(s.account.id)} onchange={() => toggle(s.account.id)} />
                {s.account.name}
              </label>
            {/each}
          </div>
        </fieldset>
      {/if}

      <div class="field">
        <span class="label">Period</span>
        <Segmented
          label="Period"
          bind:value={mode}
          items={[
            { id: 'month', label: 'One month' },
            { id: 'custom', label: 'Custom range' },
            { id: 'all', label: 'Every month' },
          ]}
        />
      </div>

      {#if mode === 'month'}
        <div class="month">
          <Button variant="ghost" size="sm" aria-label="Previous month" onclick={() => (ym = shiftMonth(ym.year, ym.month, -1))}>
            <Icon name="chevron-left" size={14} />
          </Button>
          <strong>{formatMonth(ym.year, ym.month)}</strong>
          <Button variant="ghost" size="sm" aria-label="Next month" onclick={() => (ym = shiftMonth(ym.year, ym.month, 1))}>
            <Icon name="chevron-right" size={14} />
          </Button>
          <span class="muted range mono">{range.start} → {range.end}</span>
        </div>
      {:else if mode === 'custom'}
        <div class="dates">
          <div class="field"><label for="ex-from">From</label><input id="ex-from" class="input" type="date" bind:value={from} /></div>
          <div class="field"><label for="ex-to">To</label><input id="ex-to" class="input" type="date" bind:value={to} /></div>
        </div>
      {:else}
        <p class="hint">
          {#if months.length}
            Creates {months.length} file{months.length === 1 ? '' : 's'} per account, from the 1st to the last day of each month between
            {span?.first} and {span?.last}.
          {:else}
            Nothing to export yet.
          {/if}
        </p>
      {/if}

      <div class="field">
        <span class="label">Destination</span>
        <div class="dest">
          <Button size="sm" onclick={chooseFolder}>Choose folder…</Button>
          <span class="path mono" class:muted={!dir}>{dir ?? 'No folder chosen'}</span>
        </div>
      </div>

      {#if error}<Alert tone="error">{error}</Alert>{/if}
    </div>
  {/if}

  {#snippet footer()}
    {#if results}
      <Button variant="primary" onclick={() => (open = false)}>Done</Button>
    {:else}
      {#if invalid}<span class="why muted">{invalid}</span>{/if}
      <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
      <Button variant="primary" loading={busy} disabled={!!invalid} onclick={run}><Icon name="download" size={14} /> Export</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .form,
  .results {
    display: grid;
    gap: 16px;
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
    min-width: 0;
  }
  .accts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
  }
  .month {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .month strong {
    min-width: 140px;
    text-align: center;
  }
  .range {
    margin-left: 8px;
    font-size: 12px;
  }
  .dates {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .dest {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .path {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    max-height: 240px;
    overflow: auto;
  }
  li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
  }
  li:last-child {
    border-bottom: 0;
  }
  .file {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .why {
    margin-right: auto;
    align-self: center;
    font-size: 12px;
  }
</style>
