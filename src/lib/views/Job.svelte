<script>
  import { invoke } from '@tauri-apps/api/core';
  import { openPath, openUrl } from '@tauri-apps/plugin-opener';
  import { app, refresh, STATUSES, statusLabel, base, fmtDate, toast, deleteJobs } from '../state.svelte.js';
  import { analyze, generate, compile, followUp, afterManualEdit } from '../pipeline.js';
  import DocEditor from './DocEditor.svelte';

  let { id } = $props();
  let job = $state(null);
  let msg = $state('');
  let docTab = $state('cv_en');
  let snap = $state(null);
  let posting = $state(null);
  let infoTab = $state('extracted');
  // [key, label, output file prefix, typst source]
  const DOCS = [
    ['cv_en', 'CV · English', 'CV_EN_', 'CV_en.typ'],
    ['letter_en', 'Letter · English', 'Motivation_Letter_EN_', 'Letter_en.typ'],
    ['cv_de', 'Lebenslauf · Deutsch', 'CV_DE_', 'CV_de.typ'],
    ['letter_de', 'Anschreiben · Deutsch', 'Motivation_Letter_DE_', 'Letter_de.typ']
  ];
  let editing = $state(null);
  let showLog = $state(false);
  let archiveAsk = $state(false);

  const busy = $derived(app.busy[id]);
  const res = $derived(app.results[id]);
  const pending = $derived(app.pending[id] ?? []);
  const files = $derived((res?.files ?? []).filter((f) => !DOCS.some(([k, , pre]) => pending.includes(k) && base(f).startsWith(pre))));
  const curDoc = $derived(DOCS.find((d) => d[0] === docTab));
  // jump to the English CV as soon as it is the only one available
  $effect(() => {
    const ready = DOCS.find(([k]) => !pending.includes(k));
    if (pending.includes(docTab) && ready) docTab = ready[0];
  });
  async function showPosting() {
    infoTab = 'posting';
    posting ??= await invoke('get_posting', { id });
  }
  const info = $derived(job?.info && typeof job.info === 'object' ? job.info : {});

  async function load() {
    job = await invoke('get_job', { id });
    snap = job.snapshot ? await invoke('get_snapshot', { id }) : null;
    if (!app.results[id]) {
      const out = await invoke('list_outputs', { id });
      if (out.length && !app.busy[id] && !app.pending[id]) compile(id, 0).catch(() => {});
    }
  }
  load();
  // reload whenever a pipeline step finishes
  let wasBusy = false;
  $effect(() => {
    const b = !!busy;
    if (wasBusy && !b) load();
    wasBusy = b;
  });

  const steps = $derived.by(() => {
    const extracted = !!job?.company;
    return [
      { label: 'Extract', done: extracted },
      { label: 'Company check', done: extracted, bad: job?.flagged },
      { label: 'Documents', done: files.length > 0 && !pending.length, running: pending.length > 0 },
      { label: 'Applied', done: job && !['draft', 'ready'].includes(job.status) }
    ];
  });

  async function save(patch = {}) {
    const hadSnap = !!job.snapshot;
    job = await invoke('update_job', { id, status: job.status, notes: job.notes, archived: job.archived, ...patch });
    if (job.snapshot) snap = await invoke('get_snapshot', { id });
    if (!hadSnap && job.snapshot) toast('Saved a snapshot of the documents, posting and notes you applied with.', 'ok');
    refresh();
  }
  async function updateSnapshot() {
    if (!confirm('Replace the application snapshot with the current documents and notes?')) return;
    job = await invoke('refresh_snapshot', { id });
    snap = await invoke('get_snapshot', { id });
    toast('Snapshot updated', 'ok');
  }
  async function clearChat() {
    if (!confirm('Clear the conversation history of this application?')) return;
    job = await invoke('clear_chat', { id });
  }
  async function archive(applied) {
    archiveAsk = false;
    await save({ archived: true, status: applied && ['draft', 'ready'].includes(job.status) ? 'applied' : job.status });
    toast('Archived — find it in the Tracker.', 'ok');
  }
  const remove = () => deleteJobs([id]);
  const run = (fn) => fn(id).catch(() => {});
  async function send() {
    if (!msg.trim() || busy) return;
    const m = msg; msg = '';
    await followUp(id, m).catch(() => {});
    load();
  }
  const docFiles = (prefix) => files.filter((f) => base(f).startsWith(prefix));
  const fileLabel = (f) => {
    const b = base(f);
    const d = DOCS.find(([, , pre]) => b.startsWith(pre));
    return `${d ? d[1] : b} · ${b.split('.').pop().toUpperCase()}`;
  };
</script>

{#if editing}
  <DocEditor {id} doc={editing} onclose={() => (editing = null)} onsaved={(k) => afterManualEdit(id, k).catch(() => {})} />
{:else if job}
<div class="page">
  <header class="head">
    <div class="grow">
      <div class="row muted small">
        <span>#{job.id}</span>·<span>created {fmtDate(job.created_at)}</span>
        {#if job.applied_at}·<span>applied {fmtDate(job.applied_at)}</span>{/if}
        {#if job.archived}<span class="pill">Archived</span>{/if}
      </div>
      <h2>{job.title || 'Analyzing job…'}</h2>
      <div class="row muted">
        {#if job.company}<b class="co">{job.company}</b>{/if}
        {#if job.location}<span>· {job.location}</span>{/if}
        {#if job.url}<button class="ghost sm" onclick={() => openUrl(job.url)}>↗ Open posting</button>{/if}
      </div>
    </div>
    <div class="row">
      <select class="status" bind:value={job.status} onchange={() => save()} style="--c: var(--s-{job.status})">
        {#each STATUSES as s}<option value={s}>{statusLabel(s)}</option>{/each}
      </select>
      {#if job.archived}
        <button onclick={() => save({ archived: false })}>Unarchive</button>
      {:else}
        <button onclick={() => (archiveAsk = true)}>Archive</button>
      {/if}
      <button class="ghost" title="Open folder" onclick={() => openPath(job.workdir)}>📂</button>
      <button class="ghost danger" title="Delete" onclick={remove}>🗑</button>
    </div>
  </header>

  {#if archiveAsk}
    <div class="card banner warn">
      <span class="grow">Did you send this application?</span>
      <button class="primary" onclick={() => archive(true)}>Yes, applied — archive</button>
      <button onclick={() => archive(false)}>Not applied — archive</button>
      <button class="ghost" onclick={() => (archiveAsk = false)}>Cancel</button>
    </div>
  {/if}

  <ol class="stepper">
    {#each steps as s, i}
      <li class:done={s.done} class:bad={s.bad} class:running={s.running}><span class="n">{#if s.running}<span class="spinner"></span>{:else}{s.bad ? '⚑' : s.done ? '✓' : i + 1}{/if}</span>{s.label}</li>
    {/each}
  </ol>

  {#if busy}
    <div class="card busy">
      <div class="row"><span class="spinner"></span><b class="grow">{busy}</b>
        <button class="ghost sm" onclick={() => (showLog = !showLog)}>{showLog ? 'Hide' : 'Show'} output</button></div>
      {#if showLog}<pre class="log">{(app.logs[id] || 'Waiting for output…').slice(-6000)}</pre>{/if}
    </div>
  {/if}

  {#if job.company && !busy}
    {#if job.flagged}
      <div class="banner bad"><span class="grow">⚑ Flagged — {job.flag_reason}</span>
        {#if !files.length}<button onclick={() => run(generate)}>Generate anyway</button>{/if}</div>
    {:else}
      <div class="banner ok">✓ {job.company} is not on your company list — this job is appliable.</div>
    {/if}
  {:else if !job.company && !busy}
    <div class="banner warn"><span class="grow">The job hasn't been analyzed yet (or extraction failed).</span>
      <button class="primary" onclick={() => run(analyze)}>Analyze</button></div>
  {/if}

  <div class="cols">
    <section class="card stack docs">
      <div class="row">
        <h3 class="grow">Documents</h3>
        {#if job.company}
          <button class="sm" disabled={!!busy || pending.length > 0} onclick={() => run(generate)}>{files.length ? '↻ Regenerate' : 'Generate'}</button>
          {#if files.length}<button class="sm" disabled={!!busy || pending.length > 0} onclick={() => run((i) => compile(i, 0))}>Re-render</button>{/if}
        {/if}
      </div>
      {#if pending.length}
        <div class="progress" role="status">
          <div class="bar"><i></i></div>
          <div class="row small">
            {#each DOCS as [k, l]}
              <span class="pstep" class:done={!pending.includes(k)}>
                {#if pending.includes(k)}<span class="spinner"></span>{:else}✓{/if} {l}
              </span>
            {/each}
            <span class="grow"></span>
            <span class="muted">{pending.length === DOCS.length ? 'Generating…' : 'Still working in the background — you can read the finished documents meanwhile'}</span>
          </div>
        </div>
      {/if}
      {#if res?.previews && Object.keys(res.previews).length}
        <div class="tabs">
          {#each DOCS as [k, l]}
            <button class:on={docTab === k} disabled={pending.includes(k)} title={pending.includes(k) ? 'Still being generated' : ''}
              onclick={() => (docTab = k)}>{#if pending.includes(k)}<span class="spinner"></span>{/if}{l}</button>
          {/each}
        </div>
        {#if pending.includes(docTab)}
          <div class="locked"><span class="spinner"></span> {curDoc[1]} is still being generated…</div>
        {:else}
          <div class="dl row">
            <button class="sm primary" disabled={!!busy || pending.length > 0} title="Edit this document" onclick={() => (editing = curDoc)}>✎ Edit</button>
            {#each docFiles(curDoc[2]) as f}
              <button class="sm" onclick={() => openPath(f)}>⬇ {f.split('.').pop().toUpperCase()}</button>
            {/each}
            <span class="grow"></span>
            <span class="muted small">{res.previews[docTab]?.length ?? 0} page(s)</span>
          </div>
          <div class="pages">
            {#each res.previews[docTab] ?? [] as p}<img src="data:image/png;base64,{p}" alt="page" />{/each}
          </div>
        {/if}
      {:else}
        <div class="empty">{busy ? 'Working on it…' : 'No documents yet.'}</div>
      {/if}
    </section>

    <section class="stack side">
      {#if snap}
        <div class="card stack applied">
          <div class="row"><h3 class="grow">📎 Applied with</h3>
            <button class="ghost sm" onclick={() => openPath(snap.dir)}>Folder</button>
            <button class="ghost sm" onclick={updateSnapshot}>Update</button></div>
          <p class="muted small m0">Frozen copy from {fmtDate(job.applied_at)}: the documents you sent, the job posting and your notes at that moment.</p>
          <div class="row">
            {#each snap.files as f}<button class="sm" onclick={() => openPath(f)}>{fileLabel(f)}</button>{/each}
          </div>
          <details><summary class="small">Posting & notes at application time</summary><pre class="snapmd">{snap.summary}</pre></details>
        </div>
      {/if}
      {#if info.summary}
        <div class="card stack jd">
          <div class="row"><h3 class="grow">Job description</h3>
            {#if job.url}<button class="ghost sm" onclick={() => openUrl(job.url)}>↗</button>{/if}</div>
          <div class="tabs">
            <button class:on={infoTab === 'extracted'} onclick={() => (infoTab = 'extracted')}>Extracted</button>
            <button class:on={infoTab === 'posting'} onclick={showPosting}>Original posting</button>
          </div>
          {#if infoTab === 'extracted'}
            <div class="jdbody stack">
              <p class="m0">{info.summary}</p>
              <div class="facts small">
                {#each [['Position', info.title], ['Company', info.company], ['Location', info.location], ['Type', info.employment_type], ['Salary', info.salary], ['Contact', info.contact_person], ['E-mail', info.contact_email], ['Language', info.language]] as [k, v]}
                  {#if v}<span class="muted">{k}</span><span>{v}</span>{/if}
                {/each}
              </div>
              {#each [['Responsibilities', info.responsibilities], ['Requirements', info.requirements], ['Nice to have', info.nice_to_have]] as [t, list]}
                {#if list?.length}<div><b class="small">{t}</b><ul class="small">{#each list as r}<li>{r}</li>{/each}</ul></div>{/if}
              {/each}
              {#if info.keywords?.length}<div class="row">{#each info.keywords as k}<span class="pill">{k}</span>{/each}</div>{/if}
            </div>
          {:else}
            <pre class="jdbody posting">{posting ?? 'Loading…'}</pre>
          {/if}
        </div>
      {/if}

      <div class="card stack">
        <div class="row"><h3 class="grow">Ask the AI for changes</h3>
          {#if job.chat.length}<button class="ghost sm" disabled={!!busy} onclick={clearChat}>Clear chat</button>{/if}</div>
        <div class="chat">
          {#each job.chat as m}
            <div class="msg {m.role}">
              <div class="who small">{m.role === 'user' ? 'You' : m.role === 'agent' ? 'AI' : 'Error'} <span class="muted">{m.at ?? ''}</span></div>
              <div class="txt">{m.text}</div>
            </div>
          {/each}
        </div>
        <div class="row">
          <input class="grow" bind:value={msg} disabled={!!busy || pending.length > 0 || !files.length}
            placeholder={files.length ? 'e.g. “Emphasise my Kubernetes work”, “more formal letter”' : 'Generate documents first'}
            onkeydown={(e) => e.key === 'Enter' && send()} />
          <button class="primary" disabled={!!busy || !msg.trim()} onclick={send}>Send</button>
        </div>
      </div>

      <div class="card stack">
        <h3>Notes</h3>
        <textarea rows="4" bind:value={job.notes} onblur={() => save()} placeholder="Recruiter name, salary expectations, interview dates…"></textarea>
      </div>
    </section>
  </div>
</div>
{/if}

<style>
  .head { display: flex; gap: 16px; align-items: flex-start; }
  .head h2 { margin: 2px 0; font-size: 22px; }
  .co { color: var(--text); }
  .status { width: auto; border-left: 4px solid var(--c); font-weight: 600; }
  .stepper { list-style: none; margin: 0; padding: 0; display: flex; gap: 6px; }
  .stepper li { flex: 1; display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: var(--radius-sm); background: var(--surface); border: 1px solid var(--border); color: var(--muted); font-weight: 600; font-size: 13px; }
  .stepper .n { width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center; background: var(--surface-2); font-size: 12px; }
  .stepper li.done { color: var(--text); }
  .stepper li.done .n { background: var(--ok); color: white; }
  .stepper li.bad .n { background: var(--danger); color: white; }
  .busy { border-color: var(--accent); }
  .stepper li.running .n { background: var(--accent); color: var(--accent-contrast); }
  .progress { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border-radius: var(--radius-sm); background: var(--accent-soft); }
  .bar { height: 4px; border-radius: 4px; background: var(--surface); overflow: hidden; }
  .bar i { display: block; height: 100%; width: 35%; background: var(--accent); border-radius: 4px; animation: slide 1.3s ease-in-out infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(300%); } }
  .pstep { display: inline-flex; align-items: center; gap: 5px; font-weight: 600; color: var(--muted); }
  .pstep.done { color: var(--ok); }
  .tabs button:disabled { cursor: not-allowed; }
  .tabs .spinner { width: 11px; height: 11px; margin-right: 4px; }
  .locked { display: flex; align-items: center; justify-content: center; gap: 10px; min-height: 300px; background: var(--surface-2); border-radius: var(--radius-sm); color: var(--muted); font-weight: 600; }
  .jdbody { max-height: 420px; overflow: auto; }
  .posting { white-space: pre-wrap; font: 12px/1.5 var(--mono); margin: 0; background: var(--surface-2); padding: 10px; border-radius: var(--radius-sm); }
  .log { margin: 10px 0 0; max-height: 280px; overflow: auto; font: 12px/1.45 var(--mono); white-space: pre-wrap; background: var(--surface-2); padding: 10px; border-radius: var(--radius-sm); }
  .cols { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(300px, 1fr); gap: 16px; align-items: start; }
  .pages { display: flex; flex-direction: column; gap: 12px; max-height: 70vh; overflow: auto; padding: 12px; background: var(--surface-2); border-radius: var(--radius-sm); }
  .pages img { width: 100%; background: white; box-shadow: 0 2px 10px rgb(0 0 0 / .15); border-radius: 2px; }
  .m0 { margin: 0; }
  .applied { border-color: var(--ok); }
  .snapmd { white-space: pre-wrap; font: 12px/1.45 var(--mono); max-height: 260px; overflow: auto; background: var(--surface-2); padding: 8px; border-radius: var(--radius-sm); margin: 6px 0 0; }
  .facts { display: grid; grid-template-columns: auto 1fr; gap: 3px 12px; }
  ul { margin: 6px 0 0; padding-left: 18px; }
  .chat { display: flex; flex-direction: column; gap: 8px; max-height: 340px; overflow: auto; }
  .msg { padding: 8px 10px; border-radius: var(--radius-sm); background: var(--surface-2); }
  .msg.user { background: var(--accent-soft); align-self: flex-end; max-width: 90%; }
  .msg.error { background: var(--danger-soft); }
  .who { font-weight: 700; margin-bottom: 2px; }
  .txt { white-space: pre-wrap; font-size: 13px; max-height: 160px; overflow: auto; }
  @media (max-width: 980px) { .cols { grid-template-columns: 1fr; } }
</style>
