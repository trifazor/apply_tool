import { invoke } from '@tauri-apps/api/core';
import { app, refresh, step, toast } from './state.svelte.js';
import { extractPrompt, englishPrompt, germanPrompt, syncTranslationPrompt, shortenPrompt, followUpPrompt, fixPrompt } from './prompts.js';

const agent = (id, prompt, label) => invoke('run_agent', { id, prompt, label, cont: false });

async function setStatus(id, status) {
  const j = await invoke('get_job', { id });
  if (j.status === 'draft') await invoke('update_job', { id, status, notes: j.notes, archived: j.archived });
}

/** Full flow for a freshly created job. Stops after the company check when the job is flagged. */
export async function analyze(id) {
  await step(id, 'Reading the job posting…', () => agent(id, extractPrompt(), 'Extract job information'));
  const job = await step(id, 'Checking company list…', () => invoke('ingest_job_info', { id }));
  await refresh();
  if (job.flagged) {
    toast(`⚑ ${job.company} is on your company list — review before generating.`, 'error');
    return;
  }
  toast(`✓ ${job.company || 'This job'} is appliable — generating documents…`, 'ok');
  await generate(id);
}

export const DOC_KEYS = ['cv_en', 'letter_en', 'cv_de', 'letter_de'];
const EN = ['cv_en', 'letter_en'];
const DE = ['cv_de', 'letter_de'];

/**
 * Phase 1: English CV + letter → rendered and shown immediately.
 * Phase 2: German CV + letter mirroring them; their tabs stay locked until done.
 */
export async function generate(id) {
  app.results[id] = null;
  app.pending[id] = [...DOC_KEYS];
  try {
    await step(id, 'Preparing master CV…', () => invoke('prepare_job_cv', { id }));
    await step(id, 'Writing the English CV and motivation letter…', () => agent(id, englishPrompt(), 'Generate English CV + motivation letter'));
    await compile(id, 2, EN);
    app.pending[id] = [...DE];
    await step(id, 'Translating CV and letter to German…', () => agent(id, germanPrompt(), 'Generate German CV + motivation letter'));
    await compile(id, 2, DE);
  } finally {
    delete app.pending[id];
  }
  await setStatus(id, 'ready');
  await refresh();
  toast('All documents ready ✓', 'ok');
}

/** After a manual edit of `key` (e.g. "cv_en"): re-render it with the tools, then let the AI update the other language. */
export async function afterManualEdit(id, key) {
  const [kind, lang] = key.split('_');
  const other = `${kind}_${lang === 'en' ? 'de' : 'en'}`;
  const file = (k) => `${k.startsWith('cv') ? 'CV' : 'Letter'}_${k.split('_')[1]}`;
  app.pending[id] = [other];
  try {
    await compile(id, 0, [key]);
    toast('Saved — PDF, DOCX and ODT regenerated. Updating the other language…', 'ok');
    const K = kind === 'cv' ? 'CV' : 'Letter';
    await step(id, `Updating the ${lang === 'en' ? 'German' : 'English'} ${kind === 'cv' ? 'CV' : 'letter'} from your edits…`, () =>
      agent(id, syncTranslationPrompt(K, lang, lang === 'en' ? 'de' : 'en'), `Manual edit of ${file(key)}.typ → update ${file(other)}.typ`));
    await compile(id, 2, [other]);
  } finally {
    delete app.pending[id];
  }
  toast('Both languages are up to date ✓', 'ok');
}

/**
 * Compile Typst → PDF/DOCX/ODT (optionally only some documents) and merge the previews.
 * Typst errors and a letter longer than one page are sent back to the AI (max 2 rounds).
 */
export async function compile(id, tries = 2, only = null) {
  const res = await step(id, 'Rendering PDF, DOCX and ODT…', () => invoke('compile_job', { id, only }));
  const prev = app.results[id]?.previews ?? {};
  app.results[id] = { ...res, previews: only ? { ...prev, ...res.previews } : res.previews };
  const errors = Object.entries(res.errors ?? {});
  if (errors.length && tries > 0) {
    for (const [file, error] of errors) {
      await step(id, `Fixing a Typst error in ${file}…`, () => agent(id, fixPrompt(file, error), `Fix Typst error in ${file}`));
    }
    return compile(id, tries - 1, only);
  }
  const long = res.warnings.filter((w) => w.startsWith('LETTER_TOO_LONG'));
  if (long.length && tries > 0) {
    for (const w of long) {
      const [, file, pages] = w.split(':');
      await step(id, `${file} is too long — shortening…`, () => agent(id, shortenPrompt(file, pages), `Shorten ${file} to one page (was ${pages})`));
    }
    return compile(id, tries - 1, only);
  }
  const other = res.warnings.map((w) => w.startsWith('LETTER_TOO_LONG') ? `${w.split(':')[1]} is longer than one page` : w);
  if (other.length) toast(other.join('\n'), 'error');
  return res;
}

export async function followUp(id, msg) {
  await step(id, 'Applying your changes…', () => agent(id, followUpPrompt(msg), msg));
  await compile(id);
}
