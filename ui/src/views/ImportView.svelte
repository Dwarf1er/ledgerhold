<script lang="ts">
  import { onMount } from 'svelte';
  import { api, isTauri } from '../lib/api';
  import type { ImportPreview, ImportResult, Parser } from '../lib/api';
  import Alert from '../components/Alert.svelte';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import Icon from '../components/Icon.svelte';
  import Money from '../components/Money.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Segmented from '../components/Segmented.svelte';
  import { formatDate, formatShortDate } from '../lib/format';
  import { extensionOf, SELF_DESCRIBING, suggestParser } from '../lib/parsers';
  import { links } from '../lib/router.svelte';
  import { app, errMsg, refresh, toast } from '../lib/state.svelte';

  let { accountId }: { accountId: string | null } = $props();

  const accounts = $derived(app.overview?.accounts ?? []);
  // The route parameter only preselects the account; the user's later choice wins.
  // svelte-ignore state_referenced_locally
  let selected = $state(accountId ?? '');
  let path = $state<string | null>(null);
  let parser = $state('');
  let parsers = $state<Parser[]>([]);
  let preview = $state<ImportPreview | null>(null);
  let result = $state<ImportResult | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let filter = $state<'all' | 'new' | 'duplicate'>('all');
  let limit = $state(300);
  let dragging = $state(false);

  const account = $derived(accounts.find((s) => s.account.id === selected)?.account);
  const fileName = $derived(path ? (path.split(/[\\/]/).pop() ?? path) : null);
  const needsParser = $derived(!!path && !parser && !SELF_DESCRIBING.includes(extensionOf(path)));
  const canPreview = $derived(!!selected && !!path && !needsParser);
  const rows = $derived((preview?.rows ?? []).filter((r) => filter === 'all' || r.status === filter));

  onMount(() => {
    api.parsers().then((p) => (parsers = p)).catch(() => {});
    let unlisten: (() => void) | undefined;
    if (isTauri) {
      import('@tauri-apps/api/webview')
        .then(({ getCurrentWebview }) =>
          getCurrentWebview().onDragDropEvent((e) => {
            const p = e.payload;
            if (p.type === 'enter' || p.type === 'over') dragging = true;
            else if (p.type === 'leave') dragging = false;
            else if (p.type === 'drop') {
              dragging = false;
              if (p.paths[0]) setFile(p.paths[0]);
            }
          }),
        )
        .then((u) => (unlisten = u))
        .catch(() => {});
    }
    return () => unlisten?.();
  });

  // Choosing an account suggests the matching bank format unless the user already picked one.
  let userPickedParser = false;
  $effect(() => {
    if (!userPickedParser) parser = suggestParser(account);
  });

  function setFile(p: string) {
    path = p;
    preview = null;
    result = null;
    error = null;
  }

  async function choose() {
    const p = await api.pickFile();
    if (p) setFile(p);
  }

  async function runPreview() {
    if (!canPreview || !path) return;
    busy = true;
    error = null;
    preview = null;
    try {
      preview = await api.previewImport(selected, path, parser || null);
      filter = 'all';
      limit = 300;
    } catch (e) {
      error = errMsg(e);
    } finally {
      busy = false;
    }
  }

  async function commit() {
    if (!path || !preview) return;
    busy = true;
    error = null;
    try {
      result = await api.commitImport(selected, path, parser || null);
      preview = null;
      await refresh();
      toast(`Imported ${result.newCount} transaction${result.newCount === 1 ? '' : 's'}`, 'success');
    } catch (e) {
      error = errMsg(e);
    } finally {
      busy = false;
    }
  }

  async function undo() {
    if (!result?.batchId) return;
    try {
      const n = await api.undoBatch(result.batchId);
      toast(`Removed ${n} transaction${n === 1 ? '' : 's'}`, 'success');
      result = null;
      path = null;
      await refresh();
    } catch (e) {
      error = errMsg(e);
    }
  }

  function again() {
    path = null;
    preview = null;
    result = null;
    error = null;
  }
</script>

<PageHeader title="Import statements" />

{#if accounts.length === 0}
  <EmptyState title="Add an account first">
    Statements are imported into an account, so create one before importing.
    {#snippet action()}<Button variant="primary" href={links.settings()}>Go to settings</Button>{/snippet}
  </EmptyState>
{:else if result}
  <div class="card done">
    <div class="check"><Icon name="check" size={20} /></div>
    <h2>Import complete</h2>
    <p class="muted">
      {result.newCount} new transaction{result.newCount === 1 ? '' : 's'} added to <strong>{account?.name}</strong>.
      {#if result.duplicateCount}{result.duplicateCount} duplicate{result.duplicateCount === 1 ? '' : 's'} skipped.{/if}
      {#if result.errorCount}<span class="warn"> {result.errorCount} row{result.errorCount === 1 ? '' : 's'} could not be read.</span>{/if}
    </p>
    {#if result.batchId === null}<p class="muted">Everything in this file was already imported, so nothing changed.</p>{/if}
    <div class="actions">
      <Button variant="primary" href={links.account(selected)}>View account</Button>
      <Button onclick={again}>Import another file</Button>
      {#if result.batchId}<Button variant="ghost" onclick={undo}><Icon name="undo" size={14} /> Undo</Button>{/if}
    </div>
  </div>
{:else}
  <div class="card setup">
    <div class="grid">
      <div class="field">
        <label for="imp-acct">Account</label>
        <select id="imp-acct" class="select" bind:value={selected} onchange={() => { preview = null; error = null; }}>
          <option value="" disabled>Choose an account…</option>
          {#each accounts as s (s.account.id)}
            <option value={s.account.id}>{s.account.name} · {s.account.institutionName}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <label for="imp-fmt">Format</label>
        <select
          id="imp-fmt"
          class="select"
          bind:value={parser}
          onchange={() => { userPickedParser = true; preview = null; }}
        >
          <option value="">Detect automatically</option>
          {#each parsers as p (p.id)}<option value={p.id}>{p.description}</option>{/each}
        </select>
      </div>
    </div>

    <button type="button" class="drop" class:dragging onclick={choose}>
      <Icon name="file" size={20} />
      {#if fileName}
        <span class="fname mono">{fileName}</span>
        <span class="muted">Click to choose a different file</span>
      {:else}
        <span><strong>Choose a statement</strong>{#if isTauri} or drop it here{/if}</span>
        <span class="muted">CSV, PDF, OFX, QFX or QBO</span>
      {/if}
    </button>

    {#if needsParser}
      <Alert tone="warn">CSV files don't say which bank they come from. Choose the format above.</Alert>
    {/if}
    {#if error}<Alert tone="error">{error}</Alert>{/if}

    <div class="go">
      <Button variant="primary" loading={busy && !preview} disabled={!canPreview} onclick={runPreview}>Preview import</Button>
      <span class="hint">Nothing is saved until you confirm.</span>
    </div>
  </div>

  {#if preview}
    <section class="preview" aria-label="Import preview">
      <div class="sum">
        <div class="pill"><Badge tone="green">{preview.newCount} new</Badge></div>
        <div class="pill"><Badge>{preview.duplicateCount} duplicate{preview.duplicateCount === 1 ? '' : 's'}</Badge></div>
        {#if preview.errors.length}<div class="pill"><Badge tone="amber">{preview.errors.length} unreadable</Badge></div>{/if}
        <span class="meta muted">
          {preview.fileName}{#if preview.period} · {formatShortDate(preview.period[0])} – {formatDate(preview.period[1])}{/if}
          {#if preview.accountHint} · {preview.accountHint}{/if}
        </span>
      </div>

      {#each preview.warnings as w}<Alert tone="warn">{w}</Alert>{/each}
      {#if preview.fileAlreadyImported}
        <Alert tone="info">This exact file was imported before. Duplicates are skipped automatically.</Alert>
      {/if}

      {#if preview.rows.length}
        <div class="bar">
          <Segmented
            label="Show"
            size="sm"
            bind:value={filter}
            items={[
              { id: 'all', label: `All ${preview.rows.length}` },
              { id: 'new', label: `New ${preview.newCount}` },
              { id: 'duplicate', label: `Duplicates ${preview.duplicateCount}` },
            ]}
          />
        </div>
        <div class="card tbl">
          <table>
            <thead><tr><th class="d">Date</th><th>Description</th><th class="n">Amount</th><th class="s">Status</th></tr></thead>
            <tbody>
              {#each rows.slice(0, limit) as r, i (i)}
                <tr class:dup={r.status === 'duplicate'}>
                  <td class="d muted">{formatShortDate(r.date)}</td>
                  <td class="desc">{r.description}</td>
                  <td class="n"><Money minor={r.amount} currency={preview.currency} tint /></td>
                  <td class="s">{#if r.status === 'new'}<Badge tone="green">New</Badge>{:else}<Badge>Duplicate</Badge>{/if}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if rows.length > limit}
          <div class="more"><Button size="sm" onclick={() => (limit += 300)}>Show more ({rows.length - limit} remaining)</Button></div>
        {/if}
      {/if}

      {#if preview.errors.length}
        <details class="errs">
          <summary>{preview.errors.length} row{preview.errors.length === 1 ? '' : 's'} could not be read</summary>
          <ul>
            {#each preview.errors as e}<li><span class="mono muted">{e.rowRef}</span> {e.message}</li>{/each}
          </ul>
        </details>
      {/if}

      <div class="confirm">
        <Button variant="primary" loading={busy} disabled={preview.newCount === 0} onclick={commit}>
          Import {preview.newCount} transaction{preview.newCount === 1 ? '' : 's'}
        </Button>
        <Button variant="ghost" onclick={() => (preview = null)}>Cancel</Button>
        {#if preview.newCount === 0}<span class="hint">Nothing new in this file.</span>{/if}
      </div>
    </section>
  {/if}
{/if}

<style>
  .setup {
    display: grid;
    gap: 16px;
    padding: 20px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  .drop {
    display: grid;
    justify-items: center;
    gap: 4px;
    padding: 28px 16px;
    border: 1px dashed var(--line-2);
    border-radius: var(--radius-lg);
    background: var(--bg-2);
    color: var(--fg-2);
    text-align: center;
    transition: border-color 0.12s, background 0.12s;
  }
  .drop:hover,
  .drop.dragging {
    border-color: var(--fg);
    background: var(--bg-3);
  }
  .fname {
    font-size: 14px;
    color: var(--fg);
  }
  .go {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .preview {
    display: grid;
    gap: 12px;
    margin-top: 32px;
  }
  .sum {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .meta {
    margin-left: 8px;
    font-size: 13px;
  }
  .tbl {
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
    height: 38px;
    border-bottom: 1px solid var(--line);
  }
  tbody tr:last-child td {
    border-bottom: 0;
  }
  tr.dup td:not(.s) {
    color: var(--fg-4);
  }
  .d {
    width: 100px;
    white-space: nowrap;
  }
  .n {
    text-align: right;
    white-space: nowrap;
  }
  .s {
    width: 100px;
    text-align: right;
  }
  .desc {
    max-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more {
    display: flex;
    justify-content: center;
  }
  .errs {
    padding: 10px 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--amber-bg);
    color: var(--amber);
    font-size: 13px;
  }
  .errs summary {
    cursor: pointer;
    font-weight: 500;
  }
  .errs ul {
    margin: 8px 0 0;
    padding-left: 18px;
    display: grid;
    gap: 4px;
  }
  .confirm {
    position: sticky;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 0;
    background: linear-gradient(to top, var(--bg) 70%, transparent);
  }
  .done {
    display: grid;
    justify-items: center;
    gap: 8px;
    padding: 48px 24px;
    text-align: center;
  }
  .check {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--green-bg);
    color: var(--green);
  }
  .warn {
    color: var(--amber);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  @media (max-width: 700px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
