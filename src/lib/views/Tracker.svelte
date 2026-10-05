<script>
  import { invoke } from '@tauri-apps/api/core';
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { app, go, refresh, STATUSES, statusLabel, fmtDate, toast, deleteJobs } from '../state.svelte.js';

  let q = $state('');
  let statuses = $state(new Set());
  let flag = $state('all'); // all | flagged | ok
  let arch = $state('active'); // active | archived | all
  let from = $state('');
  let sort = $state({ key: 'created_at', dir: -1 });
  let layout = $state(localStorage.getItem('tracker-layout') || 'table');
  $effect(() => localStorage.setItem('tracker-layout', layout));

  const rows = $derived.by(() => {
    const needle = q.trim().toLowerCase();
    const r = app.jobs.filter((j) =>
      (!needle || `${j.title} ${j.company} ${j.location} ${j.notes}`.toLowerCase().includes(needle)) &&
      (!statuses.size || statuses.has(j.status)) &&
      (flag === 'all' || (flag === 'flagged') === j.flagged) &&
      (arch === 'all' || (arch === 'archived') === j.archived) &&
      (!from || (j.applied_at || j.created_at) >= from));
    const { key, dir } = sort;
    return r.toSorted((a, b) => String(a[key] ?? '').localeCompare(String(b[key] ?? ''), undefined, { numeric: true }) * dir);
  });

  const stats = $derived.by(() => {
    const sent = app.jobs.filter((j) => !['draft', 'ready'].includes(j.status));
    const answered = sent.filter((j) => ['interview', 'offer', 'rejected'].includes(j.status));
    return [
      ['Total', app.jobs.length],
      ['Applied', sent.length],
      ['Interviews', app.jobs.filter((j) => ['interview', 'offer'].includes(j.status)).length],
      ['Offers', app.jobs.filter((j) => j.status === 'offer').length],
      ['Response rate', sent.length ? Math.round((answered.length / sent.length) * 100) + '%' : '—']
    ];
  });

  function toggleStatus(s) {
    const n = new Set(statuses);
    n.has(s) ? n.delete(s) : n.add(s);
    statuses = n;
  }
  const sortBy = (key) => (sort = { key, dir: sort.key === key ? -sort.dir : 1 });
  const arrow = (key) => (sort.key === key ? (sort.dir > 0 ? ' ↑' : ' ↓') : '');
  const reset = () => { q = ''; statuses = new Set(); flag = 'all'; arch = 'active'; from = ''; };

  async function setStatus(j, status) {
    await invoke('update_job', { id: j.id, status, notes: j.notes, archived: j.archived });
    refresh();
  }

  async function exportCsv() {
    const path = await saveDialog({ defaultPath: `applications-${new Date().toISOString().slice(0, 10)}.csv`, filters: [{ name: 'CSV', extensions: ['csv'] }] });
    if (!path) return;
    const esc = (v) => `"${String(v ?? '').replaceAll('"', '""')}"`;
    const head = ['id', 'title', 'company', 'location', 'status', 'flagged', 'archived', 'created_at', 'applied_at', 'url', 'notes'];
    const csv = [head.join(','), ...rows.map((j) => head.map((h) => esc(j[h])).join(','))].join('\n');
    await invoke('save_text', { path, content: csv });
    toast(`Exported ${rows.length} rows`, 'ok');
  }

  // selection + bulk actions
  let selected = $state(new Set());
  const visibleIds = $derived(rows.map((j) => j.id));
  const allSelected = $derived(visibleIds.length > 0 && visibleIds.every((i) => selected.has(i)));
  function toggleSel(id) {
    const n = new Set(selected);
    n.has(id) ? n.delete(id) : n.add(id);
    selected = n;
  }
  const toggleAll = () => (selected = allSelected ? new Set() : new Set(visibleIds));
  async function bulkDelete() {
    if (await deleteJobs([...selected])) selected = new Set();
  }
  async function bulk(patch) {
    for (const j of app.jobs.filter((x) => selected.has(x.id))) {
      await invoke('update_job', { id: j.id, status: j.status, notes: j.notes, archived: j.archived, ...patch });
    }
    selected = new Set();
    refresh();
  }

  // board drag & drop
  let dragId = null;
</script>

<div class="page">
  <div class="row">
    <h2 class="grow">Application tracker</h2>
    <div class="seg">
      <button class:on={layout === 'table'} onclick={() => (layout = 'table')}>☰ Table</button>
      <button class:on={layout === 'board'} onclick={() => (layout = 'board')}>▦ Board</button>
    </div>
    <button onclick={exportCsv}>Export CSV</button>
    <button class="primary" onclick={() => go('new')}>＋ New</button>
  </div>

  <div class="stats">
    {#each stats as [k, v]}<div class="card stat"><span class="muted small">{k}</span><b>{v}</b></div>{/each}
  </div>

  <div class="card filters stack">
    <div class="row">
      <input class="grow" type="search" bind:value={q} placeholder="Search title, company, location, notes…" />
      <select bind:value={flag} style="width:auto">
        <option value="all">All companies</option><option value="ok">Appliable only</option><option value="flagged">Flagged only</option>
      </select>
      <select bind:value={arch} style="width:auto">
        <option value="active">Active</option><option value="archived">Archived</option><option value="all">Active + archived</option>
      </select>
      <label class="row small muted">since <input type="date" bind:value={from} style="width:auto" /></label>
      <button class="ghost sm" onclick={reset}>Reset</button>
    </div>
    <div class="row">
      {#each STATUSES as s}
        <button class="chip" class:on={statuses.has(s)} onclick={() => toggleStatus(s)}>
          <span class="dot" style="background: var(--s-{s})"></span>{statusLabel(s)}
          <span class="muted">{app.jobs.filter((j) => j.status === s).length}</span>
        </button>
      {/each}
    </div>
  </div>

  {#if selected.size}
    <div class="card bulk row">
      <b class="grow">{selected.size} selected</b>
      <select style="width:auto" onchange={(e) => { if (e.target.value) bulk({ status: e.target.value }); e.target.value = ''; }}>
        <option value="">Set status…</option>
        {#each STATUSES as s}<option value={s}>{statusLabel(s)}</option>{/each}
      </select>
      <button onclick={() => bulk({ archived: true })}>Archive</button>
      <button onclick={() => bulk({ archived: false })}>Unarchive</button>
      <button class="danger" onclick={bulkDelete}>🗑 Delete</button>
      <button class="ghost" onclick={() => (selected = new Set())}>Cancel</button>
    </div>
  {/if}

  {#if layout === 'table'}
    <div class="card tablewrap">
      <table>
        <thead><tr>
          <th class="sel"><input type="checkbox" checked={allSelected} onchange={toggleAll} /></th>
          {#each [['id', '#'], ['title', 'Position'], ['company', 'Company'], ['location', 'Location'], ['status', 'Status'], ['created_at', 'Created'], ['applied_at', 'Applied']] as [k, l]}
            <th onclick={() => sortBy(k)}>{l}{arrow(k)}</th>
          {/each}
          <th></th>
        </tr></thead>
        <tbody>
          {#each rows as j (j.id)}
            <tr onclick={() => go('job', j.id)} class:arch={j.archived} class:picked={selected.has(j.id)}>
              <td class="sel" onclick={(e) => e.stopPropagation()}><input type="checkbox" checked={selected.has(j.id)} onchange={() => toggleSel(j.id)} /></td>
              <td class="muted">{j.id}</td>
              <td><b>{j.title || '—'}</b>{#if j.snapshot}<span class="snap" title="Application snapshot saved">📎</span>{/if}</td>
              <td>{#if j.flagged}<span class="flag" title={j.flag_reason}>⚑</span>{/if}{j.company}</td>
              <td class="muted">{j.location}</td>
              <td onclick={(e) => e.stopPropagation()}>
                <select class="st" value={j.status} onchange={(e) => setStatus(j, e.target.value)} style="--c: var(--s-{j.status})">
                  {#each STATUSES as s}<option value={s}>{statusLabel(s)}</option>{/each}
                </select>
              </td>
              <td class="muted">{fmtDate(j.created_at)}</td>
              <td class="muted">{fmtDate(j.applied_at)}</td>
              <td class="act" onclick={(e) => e.stopPropagation()}><button class="ghost sm danger" title="Delete" onclick={() => deleteJobs([j.id])}>🗑</button></td>
            </tr>
          {:else}
            <tr><td colspan="9" class="empty">No applications match these filters.</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="board">
      {#each STATUSES.filter((s) => !statuses.size || statuses.has(s)) as s}
        <section class="col" role="list" ondragover={(e) => e.preventDefault()}
          ondrop={() => { const j = app.jobs.find((x) => x.id === dragId); if (j && j.status !== s) setStatus(j, s); }}>
          <h4><span class="dot" style="background: var(--s-{s})"></span>{statusLabel(s)}
            <span class="muted">{rows.filter((j) => j.status === s).length}</span></h4>
          {#each rows.filter((j) => j.status === s) as j (j.id)}
            <button class="kcard" draggable="true" ondragstart={() => (dragId = j.id)} onclick={() => go('job', j.id)} class:flagged={j.flagged}>
              <b>{j.title || 'Job #' + j.id}</b>
              <span>{j.company}</span>
              <span class="muted small">{fmtDate(j.applied_at || j.created_at)}{j.archived ? ' · archived' : ''}</span>
            </button>
          {/each}
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .seg { display: flex; border: 1px solid var(--border); border-radius: var(--radius-sm); overflow: hidden; }
  .seg button { border: 0; border-radius: 0; }
  .seg button.on { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .stats { display: grid; grid-template-columns: repeat(5, 1fr); gap: 10px; }
  .stat { display: flex; flex-direction: column; gap: 2px; padding: 12px 14px; }
  .stat b { font-size: 22px; }
  .filters { padding: 12px; gap: 10px; }
  .chip { border-radius: 99px; padding: 4px 11px; font-size: 12.5px; }
  .chip.on { background: var(--accent-soft); border-color: var(--accent); }
  .tablewrap { padding: 0; overflow: auto; }
  table { width: 100%; border-collapse: collapse; }
  th { text-align: left; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; color: var(--muted); padding: 10px 12px; border-bottom: 1px solid var(--border); cursor: pointer; user-select: none; white-space: nowrap; position: sticky; top: 0; background: var(--surface); }
  td { padding: 9px 12px; border-bottom: 1px solid var(--border); }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--surface-2); }
  tr.arch { opacity: .6; }
  .flag { color: var(--danger); margin-right: 6px; }
  .snap { margin-left: 6px; font-size: 12px; }
  .sel { width: 34px; padding-right: 0; }
  .act { width: 44px; padding: 0 6px; }
  .act button { opacity: 0; }
  tr:hover .act button { opacity: 1; }
  tr.picked { background: var(--accent-soft); }
  .bulk { padding: 10px 14px; border-color: var(--accent); position: sticky; top: 8px; z-index: 5; }
  .st { width: auto; padding: 3px 8px; border-left: 4px solid var(--c); font-size: 12.5px; }
  .board { display: grid; grid-auto-flow: column; grid-auto-columns: minmax(190px, 1fr); gap: 10px; overflow-x: auto; padding-bottom: 6px; }
  .col { background: var(--surface-2); border-radius: var(--radius); padding: 8px; min-height: 300px; display: flex; flex-direction: column; gap: 6px; }
  .col h4 { display: flex; gap: 7px; align-items: center; font-size: 13px; padding: 4px 4px 6px; }
  .col h4 .muted { margin-left: auto; }
  .kcard { flex-direction: column; align-items: flex-start; gap: 2px; text-align: left; white-space: normal; box-shadow: var(--shadow); }
  .kcard.flagged { border-left: 4px solid var(--danger); }
  @media (max-width: 900px) { .stats { grid-template-columns: repeat(3, 1fr); } }
</style>
