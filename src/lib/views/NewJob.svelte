<script>
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { app, go, refresh, step, toast, base } from '../state.svelte.js';
  import { analyze } from '../pipeline.js';

  let url = $state(''), text = $state('');
  let shots = $state([]); // { path, src? }
  let starting = $state(false);

  async function pick() {
    const r = await open({ multiple: true, filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }] });
    if (r) shots.push(...(Array.isArray(r) ? r : [r]).map((path) => ({ path })));
  }

  async function onPaste(e) {
    for (const item of e.clipboardData?.items ?? []) {
      if (!item.type.startsWith('image/')) continue;
      e.preventDefault();
      const file = item.getAsFile();
      const src = URL.createObjectURL(file);
      const b64 = await new Promise((res) => {
        const fr = new FileReader();
        fr.onload = () => res(String(fr.result).split(',')[1]);
        fr.readAsDataURL(file);
      });
      const path = await invoke('save_paste', { data: b64, ext: item.type.split('/')[1] || 'png' });
      shots.push({ path, src });
    }
  }

  async function start() {
    if (!text.trim() && !url.trim() && !shots.length) return toast('Paste a link, the job text or add screenshots.', 'error');
    starting = true;
    try {
      let body = text;
      const u = url.trim();
      if (u) {
        try { body += '\n\n' + (await step('new', 'Fetching link…', () => invoke('fetch_url', { url: u }))); }
        catch { toast('Could not fetch the link (LinkedIn often needs a login) — continuing with text/screenshots.', 'error'); }
      }
      const job = await invoke('create_job', { text: body, url: u, screenshots: shots.map((s) => s.path) });
      url = ''; text = ''; shots = [];
      await refresh();
      go('job', job.id);
      analyze(job.id).catch(() => {});
    } finally { starting = false; }
  }
</script>

<svelte:window onpaste={onPaste} />

<div class="page">
  <header>
    <h2>New application</h2>
    <p class="muted">Drop in a job ad — the AI extracts it, checks your company list, then tailors your CV and writes the motivation letter.</p>
  </header>

  <div class="card stack">
    <label class="field">Job link <small>LinkedIn, XING, StepStone, Indeed, company career pages…</small>
      <input bind:value={url} placeholder="https://www.stepstone.de/stellenangebote--…" />
    </label>
    <label class="field">Job description <small>Optional if you gave a link or screenshots</small>
      <textarea rows="10" bind:value={text} placeholder="Paste the job description here…"></textarea>
    </label>
    <div class="field">
      <b class="small">Screenshots</b>
      <div class="shots">
        {#each shots as s, i}
          <div class="shot">
            {#if s.src}<img src={s.src} alt="" />{:else}<span class="small">{base(s.path)}</span>{/if}
            <button class="x" aria-label="Remove" onclick={() => shots.splice(i, 1)}>✕</button>
          </div>
        {/each}
        <button class="add" onclick={pick}>
          <span class="big">＋</span><span class="small muted">Add images<br />or press Ctrl+V</span>
        </button>
      </div>
    </div>
    <div class="row">
      <span class="grow muted small">Agent: <b>{app.settings.agent}</b> · Uses your master CV and skills from Settings</span>
      <button class="primary" disabled={starting} onclick={start}>
        {#if starting}<span class="spinner"></span>{/if} Analyze & generate
      </button>
    </div>
  </div>
</div>

<style>
  header { display: flex; flex-direction: column; gap: 4px; }
  header p { margin: 0; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .shots { display: flex; flex-wrap: wrap; gap: 10px; }
  .shot, .add { width: 128px; height: 96px; border-radius: var(--radius-sm); border: 1px solid var(--border); position: relative; overflow: hidden; display: grid; place-items: center; background: var(--surface-2); text-align: center; padding: 6px; }
  .shot img { width: 100%; height: 100%; object-fit: cover; position: absolute; inset: 0; }
  .add { border-style: dashed; flex-direction: column; display: flex; justify-content: center; gap: 2px; }
  .big { font-size: 22px; color: var(--accent); }
  .x { position: absolute; top: 4px; right: 4px; padding: 1px 6px; font-size: 11px; border-radius: 99px; }
</style>
