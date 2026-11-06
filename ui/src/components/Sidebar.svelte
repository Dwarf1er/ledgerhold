<script lang="ts">
  import { app, setTheme, theme } from '../lib/state.svelte';
  import { links, router } from '../lib/router.svelte';
  import Icon from './Icon.svelte';
  import Segmented from './Segmented.svelte';

  const route = $derived(router.route);
  const accounts = $derived(
    [...(app.overview?.accounts ?? [])].sort((a, b) => a.account.name.localeCompare(b.account.name)),
  );
  let mode = $state(theme.mode);
</script>

<aside>
  <a class="brand" href={links.overview()} aria-label="Ledgerhold home">
    <span class="mark" aria-hidden="true"></span>
    <span>Ledgerhold</span>
  </a>

  <nav aria-label="Main">
    <a href={links.overview()} class:active={route.name === 'overview'}><Icon name="home" /> Overview</a>
    <a href={links.import()} class:active={route.name === 'import'}><Icon name="upload" /> Import</a>
    <a href={links.settings()} class:active={route.name === 'settings'}><Icon name="sliders" /> Settings</a>
  </nav>

  {#if accounts.length}
    <div class="section">Accounts</div>
    <nav class="accounts" aria-label="Accounts">
      {#each accounts as s (s.account.id)}
        <a
          href={links.account(s.account.id)}
          class:active={route.name === 'account' && route.id === s.account.id}
          title={s.account.name}
        >
          <Icon name={s.account.kind === 'credit_card' ? 'card' : 'bank'} size={14} />
          <span class="name">{s.account.name}</span>
        </a>
      {/each}
    </nav>
  {/if}

  <div class="foot">
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
    margin: 20px 8px 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--fg-4);
  }
  .accounts a {
    height: 30px;
    font-weight: 400;
    font-size: 13px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot {
    margin-top: auto;
    padding: 12px 4px 0;
  }
</style>
