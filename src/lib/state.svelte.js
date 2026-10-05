import { invoke } from '@tauri-apps/api/core';

export const STATUSES = ['draft', 'ready', 'applied', 'interview', 'offer', 'rejected', 'no_response'];
export const statusLabel = (s) => ({ no_response: 'No response' })[s] ?? s[0].toUpperCase() + s.slice(1);

export const app = $state({
  view: 'new',
  editing: false,
  jobId: null,
  jobs: [],
  companies: [],
  settings: {},
  busy: {}, // key (job id or 'cv') -> label
  logs: {}, // job id -> streamed agent output
  results: {}, // job id -> { files, previews, warnings } from the last compile
  pending: {}, // job id -> document keys still being generated in the background (locked in the UI)
  toasts: []
});

export const go = (view, jobId = null) => { app.view = view; app.jobId = jobId; };

export async function refresh() {
  [app.jobs, app.companies] = await Promise.all([invoke('list_jobs'), invoke('list_companies')]);
}

export function toast(text, kind = 'info') {
  const t = { id: Math.random(), text, kind };
  app.toasts.push(t);
  setTimeout(() => (app.toasts = app.toasts.filter((x) => x.id !== t.id)), kind === 'error' ? 9000 : 3500);
}

/** Run an async step, tracking a busy label under `key` and surfacing errors as toasts. */
export async function step(key, label, fn) {
  app.busy[key] = label;
  try { return await fn(); }
  catch (e) { toast(String(e).slice(0, 600), 'error'); throw e; }
  finally { delete app.busy[key]; }
}

export const base = (p) => p.split('/').pop();
export const fmtDate = (s) => (s ? new Date(s.replace(' ', 'T') + 'Z').toLocaleDateString() : '');

/** Delete jobs after confirmation; asks whether to keep the saved application snapshots. Returns true if deleted. */
export async function deleteJobs(ids) {
  const jobs = app.jobs.filter((j) => ids.includes(j.id));
  if (!jobs.length) return false;
  const what = jobs.length === 1 ? `“${jobs[0].title || 'Job #' + jobs[0].id}”` : `${jobs.length} applications`;
  if (!confirm(`Delete ${what} including chat and generated files?`)) return false;
  let keep = false;
  if (jobs.some((j) => j.snapshot)) {
    keep = !confirm('Also delete the saved application snapshot (the CV/letter you sent, posting and notes)?\n\nOK = delete it too · Cancel = keep it in ~/ApplyTool/applications');
  }
  for (const j of jobs) {
    await invoke('delete_job', { id: j.id, keepSnapshot: keep });
    delete app.results[j.id];
  }
  if (ids.includes(app.jobId)) go('tracker');
  await refresh();
  toast(`Deleted ${what}`, 'ok');
  return true;
}
