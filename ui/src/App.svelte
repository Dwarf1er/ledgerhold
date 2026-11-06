<script lang="ts">
  import { onMount } from 'svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Toasts from './components/Toasts.svelte';
  import Alert from './components/Alert.svelte';
  import Overview from './views/Overview.svelte';
  import AccountView from './views/AccountView.svelte';
  import ImportView from './views/ImportView.svelte';
  import Settings from './views/Settings.svelte';
  import Login from './views/Login.svelte';
  import { backend } from './lib/api';
  import { setUnauthorizedHandler } from './lib/unauthorized';
  import { app, applyTheme, initSession, markSignedOut, refresh, session } from './lib/state.svelte';
  import { router } from './lib/router.svelte';

  onMount(() => {
    applyTheme();
    if (backend === 'http') setUnauthorizedHandler(markSignedOut);
    initSession().then(async () => {
      if (session.authenticated) await refresh();
      else app.loading = false;
    });
    return () => setUnauthorizedHandler(null);
  });

  const route = $derived(router.route);
</script>

{#if session.checked && !session.authenticated}
  <Login />
{:else}
<div class="shell">
  <Sidebar />
  <main>
    <div class="page">
      {#if app.error}
        <Alert tone="error">Could not load your data: {app.error}</Alert>
      {:else if app.loading || !app.overview}
        <div class="loading muted" role="status">Loading…</div>
      {:else if route.name === 'overview'}
        <Overview />
      {:else if route.name === 'account'}
        {#key route.id}
          <AccountView id={route.id} tab={route.tab} />
        {/key}
      {:else if route.name === 'import'}
        <ImportView accountId={route.accountId} />
      {:else if route.name === 'settings'}
        <Settings />
      {/if}
    </div>
  </main>
</div>
{/if}
<Toasts />

<style>
  .shell {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100%;
  }
  main {
    overflow-y: auto;
    min-width: 0;
  }
  .page {
    max-width: 1080px;
    margin: 0 auto;
    padding: 32px 32px 64px;
  }
  .loading {
    padding: 48px 0;
  }
  @media (max-width: 760px) {
    .shell {
      grid-template-columns: 1fr;
    }
    .shell > :global(aside) {
      display: none;
    }
    .page {
      padding: 20px 16px 48px;
    }
  }
</style>
