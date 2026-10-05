<script>
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { app, refresh, toast } from '../state.svelte.js';

  let q = $state('');
  let text = $state('');
  const list = $derived(app.companies.filter((c) => `${c.name} ${c.note}`.toLowerCase().includes(q.trim().toLowerCase())));

  async function add() {
    const n = await invoke('add_companies', { text });
    text = '';
    await refresh();
    toast(`Added ${n} compan${n === 1 ? 'y' : 'ies'}`, 'ok');
  }
  async function importCsv() {
    const path = await open({ filters: [{ name: 'CSV', extensions: ['csv', 'txt', 'tsv'] }] });
    if (!path) return;
    try {
      const n = await invoke('import_companies_csv', { path });
      await refresh();
      toast(`Imported ${n} new compan${n === 1 ? 'y' : 'ies'}`, 'ok');
    } catch (e) { toast(String(e), 'error'); }
  }
  async function del(id) { await invoke('delete_company', { id }); refresh(); }
  async function clearAll() {
    if (!confirm(`Remove all ${app.companies.length} companies?`)) return;
    await invoke('clear_companies'); refresh();
  }
</script>

<div class="page">
  <div class="row">
    <div class="grow">
      <h2>Company list</h2>
      <p class="muted m0">Jobs from these companies are <b>flagged</b> (e.g. already applied, blacklisted, current employer). Matching ignores legal forms like GmbH, AG, Inc.</p>
    </div>
    <button onclick={importCsv}>⇪ Import CSV</button>
    {#if app.companies.length}<button class="ghost danger" onclick={clearAll}>Clear all</button>{/if}
  </div>

  <div class="grid">
    <div class="card stack">
      <h3>Add companies</h3>
      <textarea rows="7" bind:value={text} placeholder={'One per line, optional note after ;\nAcme GmbH; applied March 2025\nInitech AG'}></textarea>
      <button class="primary" disabled={!text.trim()} onclick={add}>Add</button>
      <p class="muted small m0">CSV import: the first column (or a column named <i>company / name / firma / unternehmen</i>) is the company name; other columns become the note. Delimiter <code>,</code> <code>;</code> or tab is detected automatically.</p>
    </div>

    <div class="card stack list">
      <div class="row"><h3 class="grow">{app.companies.length} companies</h3>
        <input type="search" bind:value={q} placeholder="Search…" style="width: 220px" /></div>
      <ul>
        {#each list as c (c.id)}
          <li><span class="grow"><b>{c.name}</b>{#if c.note}<span class="muted small"> — {c.note}</span>{/if}</span>
            <button class="ghost sm danger" onclick={() => del(c.id)}>Remove</button></li>
        {:else}
          <li class="empty">No companies{q ? ' match' : ' yet'}.</li>
        {/each}
      </ul>
    </div>
  </div>
</div>

<style>
  .m0 { margin: 0; }
  .grid { display: grid; grid-template-columns: 340px 1fr; gap: 16px; align-items: start; }
  ul { list-style: none; margin: 0; padding: 0; max-height: 65vh; overflow: auto; }
  li { display: flex; align-items: center; gap: 8px; padding: 7px 4px; border-bottom: 1px solid var(--border); }
  li .sm { opacity: 0; }
  li:hover .sm { opacity: 1; }
  @media (max-width: 860px) { .grid { grid-template-columns: 1fr; } }
</style>
