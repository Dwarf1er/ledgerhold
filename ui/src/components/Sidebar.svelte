<script lang="ts">
  import { api } from '../lib/api';
  import type { AccountSummary } from '../lib/api';
  import { kindLabel, maskNumber } from '../lib/format';
  import Avatar from './Avatar.svelte';
  import { app, markSignedOut, session, setTheme, theme } from '../lib/state.svelte';
  import { links, router } from '../lib/router.svelte';
  import Icon from './Icon.svelte';
  import Segmented from './Segmented.svelte';

  const route = $derived(router.route);
  // Accounts grouped under their bank, banks and accounts both alphabetical.
  const groups = $derived.by(() => {
    const byBank = new Map<string, { id: string; name: string; accounts: AccountSummary[] }>();
    for (const s of app.overview?.accounts ?? []) {
      const g = byBank.get(s.account.institutionId) ?? { id: s.account.institutionId, name: s.account.institutionName, accounts: [] };
      g.accounts.push(s);
      byBank.set(s.account.institutionId, g);
    }
    return [...byBank.values()]
      .map((g) => ({ ...g, accounts: g.accounts.sort((a, b) => a.account.name.localeCompare(b.account.name)) }))
      .sort((a, b) => a.name.localeCompare(b.name));
  });
  let mode = $state(theme.mode);

  const people = $derived(app.overview?.people ?? []);
  const ownersOf = (s: AccountSummary) => s.account.ownerIds.map((id) => people.find((p) => p.id === id)).filter((p) => !!p);

  /** Everything that tells accounts apart, so a short name is enough in the row. */
  function describe(s: AccountSummary): string {
    const a = s.account;
    const owners = ownersOf(s).map((p) => p.name);
    return [
      a.name,
      [a.institutionName, kindLabel(a.kind), a.currency, maskNumber(a.number)].filter(Boolean).join(' · '),
      owners.length ? `Owner${owners.length > 1 ? 's' : ''}: ${owners.join(', ')}` : 'No owner',
    ].join('\n');
  }

  async function signOut() {
    try {
      await api.logout();
    } finally {
      markSignedOut();
    }
  }
</script>

<aside>
  <a class="brand" href={links.overview()} aria-label="Ledgerhold home">
    <span class="mark" aria-hidden="true"></span>
    <span>Ledgerhold</span>
  </a>

  <nav aria-label="Main">
    <a href={links.overview()} class:active={route.name === 'overview'}><Icon name="home" /> Overview</a>
    <a href={links.import()} class:active={route.name === 'import'}><Icon name="download" /> Import</a>
    <a href={links.settings()} class:active={route.name === 'settings'}><Icon name="sliders" /> Settings</a>
  </nav>

  {#each groups as g, i (g.id)}
    <div class="section" class:first={i === 0}>{i === 0 ? 'Accounts · ' : ''}{g.name}</div>
    <nav class="accounts" aria-label={`${g.name} accounts`}>
      {#each g.accounts as s (s.account.id)}
        <a
          href={links.account(s.account.id)}
          class:active={route.name === 'account' && route.id === s.account.id}
          title={describe(s)}
        >
          <Icon name={s.account.kind === 'credit_card' ? 'card' : 'bank'} size={14} />
          <span class="name">{s.account.name}</span>
          <span class="owners">{#each ownersOf(s) as p (p.id)}<Avatar name={p.name} color={p.color} size={16} />{/each}</span>
        </a>
      {/each}
    </nav>
  {/each}

  <div class="foot">
    {#if session.authRequired}
      <button type="button" class="signout" onclick={signOut}>Sign out</button>
    {/if}
    <Segmented
      label="Theme"
      size="sm"
      bind:value={mode}
      onchange={(m) => setTheme(m as 'system' | 'light' | 'dark')}
      items={[
        { id: 'system', icon: monitor, title: 'System theme' },
        { id: 'light', icon: sun, title: 'Light theme' },
        { id: 'dark', icon: moon, title: 'Dark theme' },
      ]}
    />
  </div>
</aside>

{#snippet monitor()}<Icon name="monitor" size={14} />{/snippet}
{#snippet sun()}<Icon name="sun" size={14} />{/snippet}
{#snippet moon()}<Icon name="moon" size={14} />{/snippet}

<style>
  aside {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 240px;
    height: 100%;
    padding: 16px 12px;
    border-right: 1px solid var(--line);
    background: var(--bg-2);
    overflow-y: auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px 16px;
    font-weight: 600;
    letter-spacing: -0.02em;
    font-size: 15px;
  }
  .mark {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    background: var(--fg);
    position: relative;
  }
  .mark::after {
    content: '';
    position: absolute;
    left: 4px;
    right: 4px;
    top: 5px;
    height: 2px;
    background: var(--bg);
    box-shadow: 0 4px 0 var(--bg);
  }
  nav {
    display: grid;
    gap: 2px;
  }
  nav a {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 8px;
    border-radius: var(--radius);
    color: var(--fg-3);
    font-weight: 500;
  }
  nav a:hover {
    background: var(--bg-3);
    color: var(--fg);
  }
  nav a.active {
    background: var(--bg-3);
    color: var(--fg);
  }
  .section {
    margin: 14px 8px 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--fg-4);
  }
  .section.first {
    margin-top: 20px;
  }
  .accounts a {
    height: 30px;
    font-weight: 400;
    font-size: 13px;
  }
  .accounts a :global(svg) {
    flex: none;
  }
  .owners {
    display: flex;
    flex: none;
    margin-left: auto;
    padding-left: 6px;
  }
  .owners :global(.avatar + .avatar) {
    margin-left: -4px;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: auto;
    padding: 12px 4px 0;
  }
  .signout {
    border: 0;
    background: none;
    padding: 4px 6px;
    border-radius: var(--radius);
    color: var(--fg-3);
    font-size: 13px;
  }
  .signout:hover {
    background: var(--bg-3);
    color: var(--fg);
  }
</style>
