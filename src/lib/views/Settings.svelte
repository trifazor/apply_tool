<script>
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { app, step, toast, base } from '../state.svelte.js';
  import { theme, applyTheme } from '../theme.svelte.js';
  import { importCvPrompt } from '../prompts.js';

  let tab = $state('general');
  const set = async (key, value) => { app.settings[key] = value; await invoke('set_setting', { key, value }); };
  const resetKey = async (key) => (app.settings[key] = await invoke('reset_setting', { key }));

  // ---- master CV / letter template ----
  let cvFile = $state('master.typ');
  let cvText = $state(''), cvSaved = $state('');
  let pages = $state([]);
  const cvBusy = $derived(app.busy.cv);
  async function loadCv() {
    cvText = cvSaved = await invoke('read_cv_file', { name: cvFile });
    preview();
  }
  async function saveCv() {
    await invoke('write_cv_file', { name: cvFile, content: cvText });
    cvSaved = cvText;
    preview();
  }
  async function preview() {
    try { pages = await invoke('preview_cv_file', { name: cvFile }); }
    catch (e) { toast('Typst error:\n' + e, 'error'); }
  }
  async function restoreCv() {
    if (!confirm(`Replace ${cvFile} with the built-in sample?`)) return;
    cvText = cvSaved = await invoke('restore_default_cv_file', { name: cvFile });
    preview();
  }
  async function importCv() {
    const path = await open({ filters: [{ name: 'CV', extensions: ['docx', 'odt', 'pdf', 'md', 'rtf', 'html'] }] });
    if (!path) return;
    try {
      const src = await step('cv', 'Copying CV…', () => invoke('import_cv_source', { path }));
      await step('cv', 'AI is converting your CV to Typst… (this can take a minute)', () => invoke('run_cv_agent', { prompt: importCvPrompt(base(src)) }));
      cvFile = 'master.typ';
      await loadCv();
      toast('Master CV imported ✓ — check the preview and fix anything by hand if needed.', 'ok');
    } catch {}
  }
  $effect(() => { if (tab === 'cv') loadCv(); });

  // ---- skills ----
  let skills = $state([]);
  let cur = $state(null); // { name, content, original }
  async function loadSkills(select) {
    skills = await invoke('list_skills');
    const s = skills.find((x) => x.name === select) ?? skills[0];
    cur = s ? { ...s, original: s.content } : null;
  }
  async function saveSkill() {
    const name = await invoke('save_skill', { name: cur.name, content: cur.content });
    if (cur.original !== undefined && name !== cur.prevName && cur.prevName) await invoke('delete_skill', { name: cur.prevName });
    await loadSkills(name);
    toast('Skill saved', 'ok');
  }
  function newSkill() {
    const name = `${String(skills.length + 1).padStart(2, '0')}-custom.md`;
    cur = { name, content: '# New skill\n- …', original: '' };
  }
  async function delSkill() {
    if (!confirm(`Delete ${cur.name}?`)) return;
    await invoke('delete_skill', { name: cur.name }).catch(() => {});
    loadSkills();
  }
  async function restoreSkills() {
    if (!confirm('Restore the 4 built-in skill files? (Your other skills are kept; built-in ones are overwritten.)')) return;
    await invoke('restore_default_skills');
    loadSkills(cur?.name);
  }
  $effect(() => { if (tab === 'skills') loadSkills(); });

  // ---- models ----
  let models = $state({}); // agent -> string[]
  let modelsLoading = $state(false);
  const curAgent = $derived(app.settings.agent);
  const modelKey = $derived(`model_${curAgent}`);
  async function loadModels(force = false) {
    const a = curAgent;
    if (!a || (models[a] && !force)) return;
    modelsLoading = true;
    try { models[a] = await invoke('list_models', { agent: a }); }
    catch (e) { models[a] = []; toast(`Could not list ${a} models:\n${e}`, 'error'); }
    finally { modelsLoading = false; }
  }
  $effect(() => { if (tab === 'general' && curAgent) loadModels(); });

  const themes = [
    ['default', 'light', 'Default'], ['default', 'dark', 'Default dark'],
    ['kitty', 'light', 'Kitty'], ['kitty', 'dark', 'Kitty dark']
  ];
  const agents = [['claude', 'Claude Code', 'claude'], ['codex', 'OpenAI Codex', 'codex'], ['opencode', 'OpenCode', 'opencode']];
</script>

<svelte:window onkeydown={(e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === 's') {
    e.preventDefault();
    if (tab === 'cv' && cvText !== cvSaved) saveCv();
    if (tab === 'skills' && cur) saveSkill();
  }
}} />

<div class="page">
  <h2>Settings</h2>
  <div class="tabs">
    {#each [['general', 'General'], ['cv', 'Master CV'], ['skills', 'Skills'], ['agent', 'Agent commands']] as [k, l]}
      <button class:on={tab === k} onclick={() => (tab = k)}>{l}</button>
    {/each}
  </div>

  {#if tab === 'general'}
    <div class="card stack">
      <h3>AI agent</h3>
      <p class="muted small m0">Runs the CLI installed on your computer with your own login/API key.</p>
      <div class="choices">
        {#each agents as [k, l, bin]}
          <button class="choice" class:on={app.settings.agent === k} onclick={() => set('agent', k)}>
            <b>{l}</b><code class="small muted">{bin}</code>
          </button>
        {/each}
      </div>
      <div class="row model">
        <label class="field grow">Model
          <small>{curAgent === 'opencode' ? 'OpenCode Go models (opencode models opencode-go)' : curAgent === 'codex' ? 'From codex debug models' : 'Claude Code aliases and model ids'} · empty = the CLI's default</small>
          <div class="row">
            <select class="grow" value={app.settings[modelKey] ?? ''} onchange={(e) => set(modelKey, e.target.value)}>
              <option value="">Default</option>
              {#each models[curAgent] ?? [] as m}<option value={m}>{m}</option>{/each}
              {#if app.settings[modelKey] && !(models[curAgent] ?? []).includes(app.settings[modelKey])}
                <option value={app.settings[modelKey]}>{app.settings[modelKey]}</option>
              {/if}
            </select>
            <input style="width: 220px" placeholder="…or type a model id" value={app.settings[modelKey] ?? ''}
              onchange={(e) => set(modelKey, e.target.value.trim())} />
            <button disabled={modelsLoading} onclick={() => loadModels(true)}>{#if modelsLoading}<span class="spinner"></span>{:else}↻{/if} Refresh</button>
          </div>
        </label>
      </div>
    </div>
    <div class="card stack">
      <h3>Appearance</h3>
      <div class="choices">
        {#each themes as [name, mode, label]}
          <button class="choice swatch" data-theme={name} data-mode={mode}
            class:on={theme.name === name && (theme.mode === mode || theme.mode === 'system')}
            onclick={() => { theme.name = name; theme.mode = mode; applyTheme(); }}>
            <span class="sw"><i></i><i></i><i></i></span><b>{name === 'kitty' ? '🎀 ' : ''}{label}</b>
          </button>
        {/each}
      </div>
      <label class="row small"><input type="checkbox" checked={theme.mode === 'system'}
        onchange={(e) => { theme.mode = e.target.checked ? 'system' : document.documentElement.dataset.mode; applyTheme(); }} />
        Follow system light/dark mode</label>
    </div>
    <div class="card row">
      <span class="grow">Data folder: <code>{app.settings.root}</code><br /><span class="muted small">Database, skills, master CV and one folder per application. Put extra fonts in <code>fonts/</code>.</span></span>
      <button onclick={() => openPath(app.settings.root)}>Open</button>
    </div>

  {:else if tab === 'cv'}
    <div class="card row">
      <div class="grow">
        <h3>Master CV</h3>
        <p class="muted small m0">Your CV is stored as Typst (<code>master.typ</code>). Each application gets a tailored copy, rendered to PDF, DOCX and ODT.
          Import an existing DOCX/ODT/PDF and the AI converts it for you.</p>
      </div>
      <button class="primary" disabled={!!cvBusy} onclick={importCv}>{#if cvBusy}<span class="spinner"></span>{/if} Import my CV…</button>
    </div>
    {#if cvBusy}
      <div class="card stack"><div class="row"><span class="spinner"></span><b>{cvBusy}</b></div>
        <pre class="log">{(app.logs[0] || '').slice(-3000)}</pre></div>
    {/if}
    <div class="editor">
      <div class="card stack">
        <div class="row">
          <select bind:value={cvFile} onchange={loadCv} style="width:auto">
            <option value="master.typ">master.typ — CV</option>
            <option value="letter.typ">letter.typ — letter template</option>
          </select>
          <span class="grow muted small">{cvText !== cvSaved ? '● unsaved' : ''}</span>
          <button class="ghost sm" onclick={restoreCv}>Restore sample</button>
          <button class="primary sm" disabled={cvText === cvSaved} onclick={saveCv}>Save & preview</button>
        </div>
        <textarea class="code" spellcheck="false" bind:value={cvText}></textarea>
      </div>
      <div class="card pages">
        {#each pages as p}<img src="data:image/png;base64,{p}" alt="preview" />{:else}<div class="empty">No preview</div>{/each}
      </div>
    </div>

  {:else if tab === 'skills'}
    <p class="muted m0">Skills are Markdown rule files in <code>{app.settings.root}/skills</code>. Before every AI run they are combined into <code>SKILLS.md</code>, which the AI must follow. Edit them to change how CVs and letters are written.</p>
    <div class="skills">
      <div class="card list">
        {#each skills as s}
          <button class="ghost item" class:on={cur?.name === s.name} onclick={() => (cur = { ...s, original: s.content })}>
            <b>{s.name}</b><span class="muted small">{s.content.split('\n')[0].replace(/^#+\s*/, '')}</span>
          </button>
        {/each}
        <div class="row pad"><button class="sm" onclick={newSkill}>＋ New skill</button>
          <button class="ghost sm" onclick={restoreSkills}>Restore defaults</button></div>
      </div>
      {#if cur}
        <div class="card stack">
          <div class="row">
            <input class="grow" value={cur.name} onchange={(e) => { cur.prevName ??= cur.name; cur.name = e.target.value; }} />
            <button class="ghost sm danger" onclick={delSkill}>Delete</button>
            <button class="primary sm" onclick={saveSkill}>Save</button>
          </div>
          <textarea class="code" rows="22" spellcheck="true" bind:value={cur.content}></textarea>
        </div>
      {/if}
    </div>

  {:else if tab === 'agent'}
    <div class="card stack">
      <p class="muted small m0">Shell commands run on the host (outside the sandbox) in the application folder. The prompt is in <code>.prompt.md</code>; <code>$MODEL</code> holds the model chosen in General (empty = default). </p>
      {#each agents as [k, l]}
        <h3>{l}{app.settings.agent === k ? ' (active)' : ''}</h3>
        {#each [[`cmd_${k}`, 'New task'], [`cmd_${k}_cont`, 'Follow-up (continues the session)']] as [key, label]}
          <label class="field">{label} <button class="ghost sm reset" onclick={() => resetKey(key)}>reset</button>
            <textarea class="code" rows="2" value={app.settings[key]} onchange={(e) => set(key, e.target.value)}></textarea>
          </label>
        {/each}
      {/each}
    </div>
  {/if}
</div>

<style>
  .m0 { margin: 0; }
  .choices { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 10px; }
  .choice { flex-direction: column; align-items: flex-start; gap: 4px; padding: 12px 14px; border-radius: var(--radius); }
  .choice.on { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); }
  .swatch { background: var(--bg); color: var(--text); }
  .sw { display: flex; gap: 4px; }
  .sw i { width: 22px; height: 22px; border-radius: 6px; border: 1px solid var(--border); }
  .sw i:nth-child(1) { background: var(--surface); }
  .sw i:nth-child(2) { background: var(--accent); }
  .sw i:nth-child(3) { background: var(--accent-soft); }
  .editor { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  .editor textarea { min-height: 62vh; flex: 1; }
  .pages { display: flex; flex-direction: column; gap: 12px; max-height: 75vh; overflow: auto; background: var(--surface-2); }
  .pages img { width: 100%; background: white; box-shadow: 0 2px 10px rgb(0 0 0 / .15); }
  .log { margin: 0; max-height: 200px; overflow: auto; font: 12px/1.45 var(--mono); white-space: pre-wrap; }
  .skills { display: grid; grid-template-columns: 260px 1fr; gap: 16px; align-items: start; }
  .list { padding: 6px; display: flex; flex-direction: column; gap: 2px; }
  .item { flex-direction: column; align-items: flex-start; gap: 0; width: 100%; text-align: left; white-space: normal; }
  .item.on { background: var(--accent-soft); }
  .pad { padding: 8px 4px 2px; }
  .reset { display: inline; padding: 0 6px; font-weight: 400; }
  @media (max-width: 980px) { .editor, .skills { grid-template-columns: 1fr; } }
</style>
