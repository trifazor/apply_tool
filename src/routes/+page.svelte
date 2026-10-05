<script>
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { app, go, refresh } from '#lib/state.svelte.js';
  import { theme, applyTheme } from '#lib/theme.svelte.js';
  import NewJob from '#lib/views/NewJob.svelte';
  import Job from '#lib/views/Job.svelte';
  import Tracker from '#lib/views/Tracker.svelte';
  import Companies from '#lib/views/Companies.svelte';
  import Settings from '#lib/views/Settings.svelte';

  onMount(() => {
    applyTheme();
    invoke('get_settings').then((s) => (app.settings = s));
    refresh();
    const un = listen('job-log', ({ payload: { id, line } }) => {
      app.logs[id] = ((app.logs[id] || '') + line + '\n').slice(-30000);
    });
    return () => un.then((f) => f());
  });

  const kitty = $derived(theme.name === 'kitty');
  const recent = $derived(app.jobs.filter((j) => !j.archived).slice(0, 30));
  const modes = { system: '◐', light: '☀', dark: '☾' };
  const cycleMode = () => {
    theme.mode = { system: 'light', light: 'dark', dark: 'system' }[theme.mode];
    applyTheme();
  };
  const nav = [
    ['new', 'New application', '＋'],
    ['tracker', 'Tracker', '▦'],
    ['companies', 'Companies', '⚑'],
    ['settings', 'Settings', '⚙']
  ];
</script>

<div class="shell" class:editing={app.editing}>
  <aside>
    <div class="brand">
      <span class="logo">{kitty ? '🎀' : '◆'}</span>
      <b>Apply Tool</b>
    </div>
    <nav>
      {#each nav as [v, label, icon]}
        <button class="nav" class:on={app.view === v} onclick={() => go(v)}>
          <span class="ico">{icon}</span>{label}
          {#if v === 'tracker'}<span class="count">{app.jobs.length}</span>{/if}
        </button>
      {/each}
    </nav>
    <div class="section">Active applications</div>
    <div class="recent">
      {#each recent as j (j.id)}
        <button class="nav job" class:on={app.view === 'job' && app.jobId === j.id} onclick={() => go('job', j.id)}>
          {#if app.busy[j.id] || app.pending[j.id]}<span class="spinner"></span>{:else}<span class="dot" style="background: var(--s-{j.status})"></span>{/if}
          <span class="grow">
            <span class="t">{j.title || 'Job #' + j.id}</span>
            <span class="c">{j.flagged ? '⚑ ' : ''}{j.company || '…'}</span>
          </span>
        </button>
      {:else}
        <p class="muted small pad">No applications yet.</p>
      {/each}
    </div>
    <div class="foot">
      <span class="muted small">{app.settings.agent ?? ''}</span>
      <button class="ghost sm" title="Theme: {theme.name}" onclick={() => { theme.name = kitty ? 'default' : 'kitty'; applyTheme(); }}>{kitty ? '🎀' : '◆'}</button>
      <button class="ghost sm" title="Mode: {theme.mode}" onclick={cycleMode}>{modes[theme.mode]}</button>
    </div>
  </aside>

  <main>
    {#if app.view === 'new'}<NewJob />
    {:else if app.view === 'job' && app.jobId}{#key app.jobId}<Job id={app.jobId} />{/key}
    {:else if app.view === 'tracker'}<Tracker />
    {:else if app.view === 'companies'}<Companies />
    {:else if app.view === 'settings'}<Settings />
    {/if}
  </main>

  <div class="toasts">
    {#each app.toasts as t (t.id)}<div class="toast {t.kind}">{t.text}</div>{/each}
  </div>
</div>

<style>
  .shell { display: grid; grid-template-columns: 250px 1fr; height: 100vh; }
  .shell.editing { grid-template-columns: minmax(0, 1fr); }
  .shell.editing > aside { display: none; }
  .shell.editing > main { grid-column: 1 / -1; overflow: hidden; }
  aside { background: var(--sidebar); border-right: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; padding: 14px 10px; gap: 4px; }
  .brand { display: flex; align-items: center; gap: 9px; padding: 4px 8px 14px; font-size: 16px; }
  .logo { width: 30px; height: 30px; border-radius: 9px; display: grid; place-items: center; background: var(--accent); color: var(--accent-contrast); font-size: 15px; }
  :global([data-theme='kitty']) .logo { background: var(--accent-soft); font-size: 18px; border-radius: 50%; }
  nav { display: flex; flex-direction: column; gap: 2px; }
  .nav { border: 0; background: none; width: 100%; justify-content: flex-start; padding: 8px 10px; gap: 10px; text-align: left; }
  .nav.on { background: var(--sidebar-active); font-weight: 600; }
  .nav:hover:not(.on) { background: var(--surface-2); }
  .ico { width: 18px; text-align: center; color: var(--accent); }
  .count { margin-left: auto; font-size: 11.5px; color: var(--muted); }
  .section { font-size: 11px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--muted); padding: 16px 10px 6px; }
  .recent { flex: 1; overflow: auto; display: flex; flex-direction: column; gap: 1px; }
  .job { align-items: center; }
  .job .t, .job .c { display: block; overflow: hidden; text-overflow: ellipsis; }
  .job .c { font-size: 12px; color: var(--muted); font-weight: 400; }
  .pad { padding: 4px 10px; }
  .foot { display: flex; align-items: center; gap: 4px; border-top: 1px solid var(--border); padding: 8px 4px 0; }
  .foot span { flex: 1; padding-left: 6px; }
  main { overflow: auto; min-width: 0; }
  .toasts { position: fixed; bottom: 16px; right: 16px; display: flex; flex-direction: column; gap: 8px; z-index: 50; max-width: 460px; }
  .toast { background: var(--surface); border: 1px solid var(--border); box-shadow: 0 8px 24px rgb(0 0 0 / .18); border-radius: var(--radius-sm); padding: 10px 14px; white-space: pre-wrap; animation: pop .18s ease-out; }
  .toast.error { border-left: 4px solid var(--danger); }
  .toast.ok { border-left: 4px solid var(--ok); }
  @keyframes pop { from { opacity: 0; transform: translateY(6px); } }
</style>
