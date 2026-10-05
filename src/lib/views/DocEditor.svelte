<script>
  import { Channel, invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { app } from '../state.svelte.js';

  /** doc = [key, label, outputPrefix, file] */
  let { id, doc, onclose, onsaved } = $props();
  const key = $derived(doc[0]);
  const label = $derived(doc[1]);
  const file = $derived(doc[3]);
  const isLetter = $derived(key.startsWith('letter'));

  let preamble = $state('');
  let body = $state('');
  let original = $state('');
  let loaded = $state(false);
  let disposed = false;
  let pages = $state([]);
  let error = $state('');
  let rendering = $state(false);
  let previewMs = $state(null);
  let saving = $state(false);
  let showLayout = $state(false);
  let ta;

  const full = () => (preamble ? preamble.replace(/\s*$/, '\n\n') : '') + body;
  const dirty = $derived((preamble, body, full() !== original));

  /** Setup code (#set/#show/#let/#import and their multi-line bodies) is hidden; the rest is the editable text. */
  function split(src) {
    const lines = src.split('\n');
    let depth = 0;
    for (let i = 0; i < lines.length; i++) {
      const t = lines[i].trim();
      const setup = t === '' || t.startsWith('//') || /^#(set|show|let|import|include)\b/.test(t);
      if (depth <= 0 && !setup) {
        return [lines.slice(0, i).join('\n').trimEnd(), lines.slice(i).join('\n')];
      }
      for (const ch of t.replace(/"(?:[^"\\]|\\.)*"/g, '')) {
        if ('([{'.includes(ch)) depth++;
        else if (')]}'.includes(ch)) depth--;
      }
    }
    return ['', src];
  }

  onMount(async () => {
    app.editing = true;
    pages = [...(app.results[id]?.previews?.[key] ?? [])];
    try {
      const source = await invoke('read_job_file', { id, name: file });
      if (disposed) return;
      [preamble, body] = split(source);
      original = full();
      loaded = true;
      render();
    } catch (e) { error = String(e); }
  });

  // Preview only — never saves and never triggers AI/translation work (that happens on "Save & regenerate").
  let timer;
  $effect(() => {
    preamble; body; loaded;
    clearTimeout(timer);
    if (loaded) timer = setTimeout(render, 120);
    return () => clearTimeout(timer);
  });
  onDestroy(() => {
    app.editing = false;
    disposed = true;
    clearTimeout(timer);
  });

  // At most one compile in flight; edits made meanwhile are rendered right after (latest content wins).
  let inFlight = false, queued = false, renderedContent = null;
  async function render() {
    if (disposed || !loaded) return;
    if (inFlight) { queued = true; return; }
    const content = full();
    if (content === renderedContent) return;
    inFlight = true;
    rendering = true;
    const started = performance.now();
    try {
      let timeout;
      const res = await Promise.race([
        new Promise((resolve, reject) => {
          const onPreview = new Channel(resolve);
          invoke('preview_live', { id, name: file, content, onPreview }).catch(reject);
        }),
        new Promise((_, reject) => { timeout = setTimeout(() => reject(new Error('Live preview did not respond within 10 seconds. Edit the text to retry.')), 10000); })
      ]).finally(() => clearTimeout(timeout));
      if (disposed) return;
      previewMs = Math.round(performance.now() - started);
      renderedContent = content;
      error = res.error ?? '';
      if (!res.error) {
        // only changed pages arrive; unchanged ones keep their image (no re-decode)
        const next = pages.slice(0, res.count);
        for (const [i, png] of res.changed) next[i] = png;
        pages = next;
      }
    } catch (e) {
      error = String(e);
    } finally {
      inFlight = false;
      rendering = false;
      if (!disposed && queued) { queued = false; render(); }
    }
  }

  // ---- toolbar (Typst markup is Markdown-like) ----
  function edit(fn) {
    if (!loaded || saving || !ta) return;
    const { selectionStart: a, selectionEnd: b } = ta;
    const [text, selA, selB] = fn(body.slice(0, a), body.slice(a, b), body.slice(b), a, b);
    body = text;
    requestAnimationFrame(() => { if (!disposed && ta) { ta.focus(); ta.setSelectionRange(selA, selB); } });
  }
  const wrap = (m) => edit((pre, sel, post, a) => [pre + m + sel + m + post, a + m.length, a + m.length + sel.length]);
  const prefix = (p) => edit((pre, sel, post, a, b) => {
    const start = pre.lastIndexOf('\n') + 1;
    const block = body.slice(start, b).split('\n')
      .map((l) => (l.startsWith(p) ? l.slice(p.length) : p + l.replace(/^(=+ |- |\+ )/, ''))).join('\n');
    return [body.slice(0, start) + block + post, start, start + block.length];
  });
  const lineBreak = () => edit((pre, sel, post, a) => [pre + sel + ' \\\n' + post, a + sel.length + 3, a + sel.length + 3]);

  async function save() {
    if (!loaded || saving) return;
    if (error && !confirm('The document currently has a Typst error and may not render. Save anyway?')) return;
    saving = true;
    try {
      const content = full();
      await invoke('write_job_file', { id, name: file, content });
      original = content;
      onsaved(key);
      onclose();
    } catch (e) { error = String(e); }
    finally { saving = false; }
  }
  function close() {
    if (saving) return;
    if (dirty && !confirm('Discard your unsaved changes?')) return;
    onclose();
  }
  function onkey(e) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key === 's') { e.preventDefault(); if (dirty) save(); }
    else if (mod && e.key === 'b') { e.preventDefault(); wrap('*'); }
    else if (mod && e.key === 'i') { e.preventDefault(); wrap('_'); }
    else if (e.key === 'Escape') { e.preventDefault(); close(); }
  }
</script>

<svelte:window onkeydown={onkey} />

<section class="document-editor" aria-label="Edit {label}">
    <header class="row">
      <div class="grow">
        <h2>✎ {label}</h2>
        <span class="muted small">After saving, PDF/DOCX/ODT are regenerated and the AI updates the {key.endsWith('en') ? 'German' : 'English'} version from your changes.</span>
      </div>
      <button class="ghost" disabled={saving} onclick={close}>← Back to job</button>
      <button class="primary" disabled={!loaded || !dirty || saving} onclick={save}>{#if saving}<span class="spinner"></span>{/if} Save & regenerate</button>
    </header>

    <fieldset class="toolbar row" disabled={!loaded || saving}>
      <button class="sm" title="Heading" onclick={() => prefix('= ')}><b>H1</b></button>
      <button class="sm" title="Section heading" onclick={() => prefix('== ')}><b>H2</b></button>
      <span class="sep"></span>
      <button class="sm" title="Bold (Ctrl+B)" onclick={() => wrap('*')}><b>B</b></button>
      <button class="sm" title="Italic (Ctrl+I)" onclick={() => wrap('_')}><i>I</i></button>
      <span class="sep"></span>
      <button class="sm" title="Bullet list" onclick={() => prefix('- ')}>• List</button>
      <button class="sm" title="Numbered list" onclick={() => prefix('+ ')}>1. List</button>
      <button class="sm" title="Line break" onclick={lineBreak}>↵ Break</button>
      <span class="grow"></span>
      <span class="muted small hint"><code>*bold*</code> <code>_italic_</code> <code>- item</code> <code>== Heading</code> · write e-mails as <code>name\@mail.com</code></span>
    </fieldset>

    <div class="split">
      <div class="stack editor">
        <textarea aria-label="Document text" disabled={!loaded || saving} bind:this={ta} bind:value={body} spellcheck="true" lang={key.endsWith('de') ? 'de' : 'en'}></textarea>
        {#if preamble}
          <details bind:open={showLayout}>
            <summary class="small muted">Layout code (advanced)</summary>
            <textarea aria-label="Layout code" disabled={saving} class="code layout" rows="8" spellcheck="false" bind:value={preamble}></textarea>
          </details>
        {/if}
      </div>
      <div class="preview">
        <div class="row small pv-head">
          {#if rendering}<span class="spinner"></span>{/if}
          <span class="grow muted">Live preview</span>
          {#if previewMs !== null}<span class="muted">{previewMs} ms</span>{/if}
          <span class:warn={isLetter && pages.length > 1}>{pages.length} page{pages.length === 1 ? '' : 's'}{isLetter && pages.length > 1 ? ' — a letter should fit on one page' : ''}</span>
        </div>
        {#if error}<pre class="err">{error}</pre>{/if}
        <div class="pages">
          {#each pages as p, i (i)}<img src="data:image/png;base64,{p}" alt="page {i + 1}" />{/each}
        </div>
      </div>
    </div>
</section>

<style>
  .document-editor { width: 100%; height: 100%; min-width: 0; min-height: 0; display: flex; flex-direction: column; gap: 12px; padding: 20px; background: var(--bg); overflow: hidden; }
  header { flex: none; flex-wrap: wrap; }
  header h2 { margin-bottom: 4px; }
  .toolbar { margin: 0; border: 0; flex: none; gap: 4px; padding: 6px; background: var(--surface-2); border-radius: var(--radius-sm); }
  .toolbar .sm { min-width: 34px; justify-content: center; }
  .sep { width: 1px; height: 20px; background: var(--border); margin: 0 4px; }
  .hint code { background: var(--surface); padding: 1px 4px; border-radius: 4px; }
  .split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); gap: 12px; }
  .editor { min-width: 0; min-height: 0; gap: 8px; }
  .editor > textarea { flex: 1; min-height: 0; font: 14px/1.6 var(--mono); resize: none; padding: 14px; }
  .layout { font-size: 12px; margin-top: 6px; }
  .preview { display: flex; flex-direction: column; min-height: 0; background: var(--surface-2); border-radius: var(--radius-sm); padding: 10px; gap: 8px; }
  .pv-head { gap: 8px; }
  .warn { color: var(--danger); font-weight: 600; }
  .err { flex: none; margin: 0; white-space: pre-wrap; font: 12px/1.4 var(--mono); background: var(--danger-soft); color: var(--danger); padding: 8px; border-radius: var(--radius-sm); max-height: 140px; overflow: auto; }
  .pages { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 12px; }
  .pages img { flex: none; width: 100%; height: auto; background: white; box-shadow: 0 2px 10px rgb(0 0 0 / .15); }
  @media (max-width: 760px) {
    .document-editor { padding: 12px; }
    .hint { display: none; }
    .split { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) minmax(0, 1fr); }
  }
</style>
