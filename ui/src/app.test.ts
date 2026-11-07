import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import App from './App.svelte';
import { __setApi } from './lib/api';
import { createMockApi } from './lib/mock';
import { parseHash, router } from './lib/router.svelte';
import { app, refresh, toasts } from './lib/state.svelte';

function go(hash: string) {
  location.hash = hash;
  router.route = parseHash(hash);
}

const main = () => within(document.querySelector('main') as HTMLElement);

beforeEach(() => {
  __setApi(createMockApi());
  app.overview = null;
  app.loading = true;
  app.error = null;
  app.person = 'all';
  toasts.length = 0;
  go('#/');
});

afterEach(() => cleanup());

describe('overview', () => {
  it('lists accounts with balances and a total per currency', async () => {
    render(App);
    expect(await main().findByText('Joint chequing')).toBeTruthy();
    expect(main().getByText('Net balance · CAD')).toBeTruthy();
    expect(main().getByText('Visa Desjardins')).toBeTruthy();
    // The USD card has no anchor yet.
    expect(main().getByText('Set starting balance')).toBeTruthy();
    expect(main().getByText('Needs a starting balance')).toBeTruthy();
  });

  it('filters by household member; joint accounts appear for both owners', async () => {
    render(App);
    await main().findByText('Joint chequing');
    await fireEvent.click(main().getByRole('radio', { name: 'Sam' }));
    expect(main().getByText('Joint chequing')).toBeTruthy();
    expect(main().getByText('Sam everyday')).toBeTruthy();
    expect(main().queryByText('Alex savings')).toBeNull();
    await fireEvent.click(main().getByRole('radio', { name: 'Alex' }));
    expect(main().getByText('Alex savings')).toBeTruthy();
    expect(main().queryByText('Sam everyday')).toBeNull();
    expect(main().getByText(/counted in full/)).toBeTruthy();
  });
});

describe('account view', () => {
  it('shows transactions with running balances and filters by search text', async () => {
    go('#/account/a-joint');
    render(App);
    expect(await main().findByText('Balance today')).toBeTruthy();
    await waitFor(() => expect(main().getAllByRole('row').length).toBeGreaterThan(20));
    const before = main().getAllByRole('row').length;
    await fireEvent.input(main().getByLabelText('Search transactions'), { target: { value: 'videotron' } });
    await waitFor(() => expect(main().getAllByRole('row').length).toBeLessThan(before), { timeout: 2000 });
    const rows = main().getAllByRole('row').slice(1);
    expect(rows.length).toBeGreaterThan(0);
    for (const r of rows) expect(r.textContent).toContain('VIDEOTRON');
    await fireEvent.click(main().getByRole('button', { name: 'Clear' }));
    await waitFor(() => expect(main().getAllByRole('row').length).toBe(before));
  });

  it('explains a reconciliation mismatch and confirms matching periods', async () => {
    go('#/account/a-joint?tab=balance');
    render(App);
    expect(await main().findByText('Reconciliation')).toBeTruthy();
    expect(main().getByText('Off by $42.18')).toBeTruthy();
    expect(main().getByText('Matches')).toBeTruthy();
    expect(main().getByText(/higher than reported/)).toBeTruthy();
  });

  it('lets you set a starting balance on an unanchored account', async () => {
    go('#/account/a-usd?tab=balance');
    render(App);
    expect(await main().findByText('Starting balance')).toBeTruthy();
    await fireEvent.input(main().getByLabelText(/Balance at end of that day/), { target: { value: '-185.50' } });
    await fireEvent.click(main().getByRole('button', { name: 'Save balance' }));
    await waitFor(() => expect(main().getByText('Balance over time')).toBeTruthy());
    expect(toasts.some((t) => t.message === 'Balance saved')).toBe(true);
    // The series loads after the tab re-renders.
    expect(await main().findByRole('img', { name: /Balance from/ })).toBeTruthy();
  });

  it('rejects an unparseable amount without calling the backend', async () => {
    go('#/account/a-usd?tab=balance');
    render(App);
    await main().findByText('Starting balance');
    await fireEvent.input(main().getByLabelText(/Balance at end of that day/), { target: { value: 'abc' } });
    await fireEvent.click(main().getByRole('button', { name: 'Save balance' }));
    expect(await main().findByText(/Enter an amount like/)).toBeTruthy();
  });

  it('undoes an import after confirmation', async () => {
    go('#/account/a-joint?tab=imports');
    render(App);
    await main().findByText('releve-desjardins.csv');
    await fireEvent.click(main().getByRole('button', { name: /Undo/ }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Undo import' }));
    await waitFor(() => expect(toasts.some((t) => /^Removed \d+ transactions?$/.test(t.message))).toBe(true));
    expect(await main().findByText('No imports yet')).toBeTruthy();
  });
});

describe('import flow', () => {
  it('previews, then imports, then offers to undo', async () => {
    go('#/import?account=a-joint');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Choose a statement/ }));
    expect(await main().findByText('releve-janvier.ofx')).toBeTruthy();
    await fireEvent.click(main().getByRole('button', { name: 'Preview import' }));
    expect(await main().findByText('5 new')).toBeTruthy();
    expect(main().getByText('4 duplicates')).toBeTruthy();
    expect(main().getByText(/1 row could not be read/)).toBeTruthy();

    await fireEvent.click(main().getByRole('radio', { name: /^New/ }));
    expect(main().getAllByText('New').length).toBe(5);

    await fireEvent.click(main().getByRole('button', { name: 'Import 5 transactions' }));
    expect(await main().findByText('Import complete')).toBeTruthy();
    expect(main().getByText(/5 new transactions added to/)).toBeTruthy();

    await fireEvent.click(main().getByRole('button', { name: /Undo/ }));
    await waitFor(() => expect(toasts.some((t) => t.message === 'Removed 5 transactions')).toBe(true));
  });

  it('asks for a format when a CSV cannot be recognised', async () => {
    go('#/import?account=a-ws');
    // Pick the account with no bank hint (Sam everyday is Wealthsimple so it suggests a parser): use a custom api.
    const api = createMockApi();
    api.pickFile = async () => '/tmp/statement.csv';
    __setApi(api);
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Choose a statement/ }));
    await main().findByText('statement.csv');
    // A parser was suggested for Wealthsimple, so previewing is allowed.
    expect((main().getByRole('button', { name: 'Preview import' }) as HTMLButtonElement).disabled).toBe(false);
    // Clearing the format to "auto" makes CSV ambiguous.
    await fireEvent.change(main().getByLabelText('Format'), { target: { value: '' } });
    expect(await main().findByText(/CSV files don't say which bank/)).toBeTruthy();
    expect((main().getByRole('button', { name: 'Preview import' }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('export dialog', () => {
  it('exports one month for the account and lists the files', async () => {
    go('#/account/a-joint');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Export OFX/ }));
    const dlg = within(await screen.findByRole('dialog'));
    const exportBtn = dlg.getByRole('button', { name: /^Export$/ }) as HTMLButtonElement;
    expect(exportBtn.disabled).toBe(true);
    expect(dlg.getByText('Choose a destination folder')).toBeTruthy();
    await fireEvent.click(dlg.getByRole('button', { name: 'Choose folder…' }));
    await waitFor(() => expect((dlg.getByRole('button', { name: /^Export$/ }) as HTMLButtonElement).disabled).toBe(false));
    await fireEvent.click(dlg.getByRole('button', { name: /^Export$/ }));
    expect(await dlg.findByText(/Wrote 1 file/)).toBeTruthy();
    expect(dlg.getByText(/joint-chequing-\d{8}-\d{8}\.ofx/)).toBeTruthy();
  });

  it('can step through months', async () => {
    go('#/account/a-joint');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Export OFX/ }));
    const dlg = within(await screen.findByRole('dialog'));
    const label = () => dlg.getByText(/^[A-Z][a-z]+ \d{4}$/).textContent;
    const start = label();
    await fireEvent.click(dlg.getByRole('button', { name: 'Next month' }));
    expect(label()).not.toBe(start);
    await fireEvent.click(dlg.getByRole('button', { name: 'Previous month' }));
    expect(label()).toBe(start);
  });

  it('closes with Escape', async () => {
    go('#/account/a-joint');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Export OFX/ }));
    const dialog = await screen.findByRole('dialog');
    await fireEvent.keyDown(dialog, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });
});

describe('settings', () => {
  it('adds a person and shows them as a possible owner', async () => {
    go('#/settings');
    render(App);
    const input = await main().findByLabelText('Person name');
    await fireEvent.input(input, { target: { value: 'Jordan' } });
    const form = within(input.closest('form') as HTMLElement);
    await fireEvent.click(form.getByRole('button', { name: /Add/ }));
    expect(await main().findByText('Jordan')).toBeTruthy();
  });

  it('renames a person and an account', async () => {
    go('#/settings');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: 'Rename Sam' }));
    const dlg = within(await screen.findByRole('dialog'));
    await fireEvent.input(dlg.getByLabelText('Name'), { target: { value: 'Samantha' } });
    await fireEvent.click(dlg.getByRole('button', { name: 'Save' }));
    expect(await main().findByText('Samantha')).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

    await fireEvent.click(main().getByRole('button', { name: 'Rename Joint chequing' }));
    const dlg2 = within(await screen.findByRole('dialog'));
    await fireEvent.input(dlg2.getByLabelText('Name'), { target: { value: 'Household' } });
    await fireEvent.click(dlg2.getByRole('button', { name: 'Save' }));
    expect(await main().findByText('Household')).toBeTruthy();
    expect(main().queryByText('Joint chequing')).toBeNull();
  });

  it('removes a person but keeps their accounts, and resets the household filter', async () => {
    go('#/settings');
    app.person = 'p-sam';
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: 'Remove Sam' }));
    const dlg = within(await screen.findByRole('dialog'));
    expect(dlg.getByText(/will stay, without them as an owner/)).toBeTruthy();
    await fireEvent.click(dlg.getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(main().queryByRole('button', { name: 'Remove Sam' })).toBeNull());
    expect(main().getByText('Sam everyday')).toBeTruthy();
    expect(app.person).toBe('all');
    expect(app.overview!.accounts.every((s) => !s.account.ownerIds.includes('p-sam'))).toBe(true);
  });

  it('asks before deleting an account, then removes it', async () => {
    go('#/settings');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: 'Delete Alex savings' }));
    const dlg = within(await screen.findByRole('dialog'));
    expect(dlg.getByText(/can't be undone/)).toBeTruthy();
    // Cancelling changes nothing.
    await fireEvent.click(dlg.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(main().getByText('Alex savings')).toBeTruthy();

    await fireEvent.click(main().getByRole('button', { name: 'Delete Alex savings' }));
    const dlg2 = within(await screen.findByRole('dialog'));
    await fireEvent.click(dlg2.getByRole('button', { name: 'Delete account' }));
    await waitFor(() => expect(main().queryByText('Alex savings')).toBeNull());
    expect(main().getByText('Joint chequing')).toBeTruthy();
  });

  it('edits a bank, and refuses to delete one that still has accounts', async () => {
    go('#/settings');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: 'Edit Wealthsimple' }));
    const dlg = within(await screen.findByRole('dialog'));
    await fireEvent.input(dlg.getByLabelText('Bank name'), { target: { value: 'Wealthsimple Cash' } });
    await fireEvent.input(dlg.getByLabelText('OFX bank ID (optional)'), { target: { value: '777' } });
    await fireEvent.click(dlg.getByRole('button', { name: 'Save' }));
    expect(await main().findByText('Wealthsimple Cash')).toBeTruthy();
    expect(main().getByText('ID 777')).toBeTruthy();
    // Accounts at the bank show the new name.
    expect(app.overview!.accounts.find((s) => s.account.id === 'a-ws')!.account.institutionName).toBe('Wealthsimple Cash');

    await fireEvent.click(main().getByRole('button', { name: 'Delete Wealthsimple Cash' }));
    const confirm = within(await screen.findByRole('dialog'));
    expect(confirm.getByText(/still has 2 accounts/)).toBeTruthy();
    await fireEvent.click(confirm.getByRole('button', { name: 'Delete bank' }));
    expect(await confirm.findByText(/still has 2 accounts; delete them first/)).toBeTruthy();
    expect(app.overview!.institutions.some((i) => i.name === 'Wealthsimple Cash')).toBe(true);
  });

  it('deletes a bank that has no accounts', async () => {
    go('#/settings');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: 'Delete Alex savings' }));
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Delete account' }));
    await waitFor(() => expect(main().queryByText('Alex savings')).toBeNull());
    await fireEvent.click(await main().findByRole('button', { name: 'Delete Wealthsimple' }));
    // Wealthsimple still has two accounts; remove them first, as the dialog says.
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    for (const name of ['Sam everyday', 'Travel card (USD)']) {
      await fireEvent.click(main().getByRole('button', { name: `Delete ${name}` }));
      await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Delete account' }));
      await waitFor(() => expect(main().queryByText(name)).toBeNull());
    }
    await fireEvent.click(main().getByRole('button', { name: 'Delete Wealthsimple' }));
    const dlg = within(await screen.findByRole('dialog'));
    expect(dlg.getByText(/will be removed/)).toBeTruthy();
    await fireEvent.click(dlg.getByRole('button', { name: 'Delete bank' }));
    await waitFor(() => expect(main().queryByRole('button', { name: 'Delete Wealthsimple' })).toBeNull());
  });

  it('creates an account with two owners', async () => {
    go('#/settings');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Add account/ }));
    const dlg = within(await screen.findByRole('dialog'));
    await fireEvent.input(dlg.getByLabelText('Name'), { target: { value: 'Holiday fund' } });
    await fireEvent.click(dlg.getByLabelText('Alex'));
    await fireEvent.click(dlg.getByLabelText('Sam'));
    await fireEvent.click(dlg.getByRole('button', { name: 'Add account' }));
    await waitFor(() => expect(main().getByText('Holiday fund')).toBeTruthy());
    await waitFor(() => expect(toasts.some((t) => t.message === 'Added Holiday fund')).toBe(true));
    expect(app.overview!.accounts.find((s) => s.account.name === 'Holiday fund')!.account.ownerIds.length).toBe(2);
  });
});

describe('dialog state', () => {
  it('keeps what you typed when the overview refreshes underneath an open dialog', async () => {
    go('#/settings');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Add account/ }));
    const dlg = within(await screen.findByRole('dialog'));
    const name = dlg.getByLabelText('Name') as HTMLInputElement;
    await fireEvent.input(name, { target: { value: 'Half typed' } });
    await refresh(); // replaces app.overview (and the institutions/people props)
    await new Promise((r) => setTimeout(r, 20));
    expect((dlg.getByLabelText('Name') as HTMLInputElement).value).toBe('Half typed');
  });
});

describe('errors', () => {
  it('shows a readable error when the backend fails to load', async () => {
    const api = createMockApi();
    api.overview = async () => {
      throw new Error('database is locked');
    };
    __setApi(api);
    render(App);
    expect(await screen.findByText(/database is locked/)).toBeTruthy();
  });
});
