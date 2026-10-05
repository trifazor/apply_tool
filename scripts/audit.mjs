// Standalone timing audit: real app prompts and real CLI exporters, isolated files.
// Does not measure Tauri IPC/Svelte paint or mutate the application database.
import * as fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { spawn } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import * as prompts from '../src/lib/prompts.js';

const args = process.argv.slice(2);
const option = (key, fallback) => args.includes(key) ? args[args.indexOf(key) + 1] : fallback;
const data = option('--data', path.join(os.homedir(), 'ApplyTool'));
const model = option('--model', 'opencode-go/gpt-6-luna');
const url = option('--url', '');
const dir = path.resolve(option('--out', path.join(os.tmpdir(), `applytool-audit-${Date.now()}`)));
const rows = [], ai = [], warnings = [];
const started = performance.now();
await fs.mkdir(dir, { recursive: true });
async function timed(name, fn) {
  const t = performance.now();
  try { return await fn(); }
  catch (e) { warnings.push(`${name}: ${e.message}`); throw e; }
  finally { const seconds = (performance.now() - t) / 1000; rows.push({ name, seconds }); console.log(`${seconds.toFixed(3)}s  ${name}`); }
}
async function command(bin, argv, cwd = dir, onLine) {
  return new Promise((resolve, reject) => {
    const p = spawn(bin, argv, { cwd, stdio: ['ignore', 'pipe', 'pipe'], detached: true });
    let out = '', err = '', pending = '';
    const timer = setTimeout(() => { process.kill(-p.pid, 'SIGTERM'); }, 600_000);
    p.stdout.on('data', b => {
      out += b; pending += b;
      const lines = pending.split('\n'); pending = lines.pop();
      for (const line of lines) onLine?.(line);
    });
    p.stderr.on('data', b => { err += b; });
    p.on('error', e => { clearTimeout(timer); reject(e); });
    p.on('close', code => { clearTimeout(timer); if (pending) onLine?.(pending); code === 0 ? resolve(out) : reject(new Error(`${bin} exited ${code}: ${err.slice(-2000)} ${out.slice(-1000)}`)); });
  });
}
async function skills() {
  const names = (await fs.readdir(path.join(data, 'skills'))).filter(n => n.endsWith('.md')).sort();
  return (await Promise.all(names.map(n => fs.readFile(path.join(data, 'skills', n), 'utf8')))).map(s => s.trim()).join('\n\n---\n\n');
}
async function expand(prompt) {
  let result = prompt;
  for (const match of prompt.matchAll(/\{\{file:([^}]+)\}\}/g)) {
    const body = await fs.readFile(path.join(dir, match[1]), 'utf8').catch(() => '(file does not exist)');
    result = result.replace(match[0], () => body);
  }
  return result.replaceAll('{{skills}}', await skills()).replaceAll('{{screenshots}}', '(none)');
}
async function agent(label, prompt, expected) {
  const t = performance.now();
  const stat = { label, model, events: {}, tokens: [], tools: [] };
  const expanded = await timed(`${label}: prepare context`, async () => {
    await fs.writeFile(path.join(dir, 'SKILLS.md'), `# SKILLS — follow ALL of these rules\n\n${await skills()}\n`);
    const body = await expand(prompt);
    await fs.writeFile(path.join(dir, '.prompt.md'), body);
    await fs.writeFile(path.join(dir, `${label}.prompt.md`), body);
    return body;
  });
  let answer = '';
  const raw = await timed(`${label}: OpenCode process`, () => command('opencode', ['run', '-m', model, '--format', 'json', expanded], dir, line => {
    let e; try { e = JSON.parse(line); } catch { return; }
    const elapsed = (performance.now() - t) / 1000;
    stat.events[e.type] ??= elapsed;
    if (e.type === 'text') answer += e.part?.text ?? '';
    if (e.type === 'tool_use') stat.tools.push(e.part?.tool);
    if (e.type === 'step_finish') stat.tokens.push(e.part?.tokens);
    if (e.type === 'error') stat.error = e.error;
    console.log(`  ${label}: ${e.type} at ${elapsed.toFixed(2)}s`);
  }));
  await fs.writeFile(path.join(dir, `${label}.events.jsonl`), raw);
  ai.push(stat);
  if (stat.error) throw new Error(JSON.stringify(stat.error));
  await timed(`${label}: parse/write file blocks`, async () => {
    let name, lines = [], written = [];
    async function flush() {
      if (!name) return;
      if (!/^[A-Za-z0-9_-]+(?:\/[A-Za-z0-9_-]+)*\.(typ|json|md)$/.test(name)) throw new Error(`Invalid output path ${name}`);
      let body = lines.join('\n').trim().replace(/^```[^\n]*\n([\s\S]*)\n```$/, '$1');
      if (name.endsWith('.typ')) body = body.replace(/([A-Za-z0-9._%+-])@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+)/g, '$1\\@$2');
      await fs.mkdir(path.dirname(path.join(dir, name)), { recursive: true });
      await fs.writeFile(path.join(dir, name), body + '\n'); written.push(name); name = null; lines = [];
    }
    for (const line of answer.split('\n')) {
      if (line.trim().startsWith('<<<FILE')) { await flush(); name = line.trim().slice(7).trim().replace(/>+$/, '').trim().replace(/^\.\//, ''); }
      else if (line.trim().startsWith('<<<END')) await flush();
      else if (name) lines.push(line);
    }
    await flush();
    for (const file of expected) if (!written.includes(file)) throw new Error(`AI did not return ${file}`);
  });
}
async function render(phase, files, round = 0) {
  const errors = [], long = [];
  for (const file of files) {
    const label = `${phase}/round${round}/${file}`;
    const source = path.join(dir, 'cv', file), base = path.join(dir, 'output', file.slice(0, -4));
    try { await timed(`${label}: PDF`, () => command('typst', ['compile', '--root', dir, '--font-path', path.join(data, 'fonts'), source, base + '.pdf'])); }
    catch (e) { errors.push([file, e.message]); continue; }
    for (const ext of ['docx', 'odt']) {
      try { await timed(`${label}: ${ext.toUpperCase()}`, () => command('pandoc', ['-f', 'typst', file, '-o', base + '.' + ext], path.join(dir, 'cv'))); }
      catch { /* App treats conversion failures as warnings. */ }
    }
    const tmp = path.join(dir, 'previews', `${phase}-${round}-${file}`);
    await timed(`${label}: preview directory`, () => fs.mkdir(tmp, { recursive: true }));
    await timed(`${label}: PNG compile`, () => command('typst', ['compile', '--root', '/', '--font-path', path.join(data, 'fonts'), '--format', 'png', '--ppi', '70', source, path.join(tmp, 'p{0p}.png')]));
    const count = await timed(`${label}: read/base64/count pages`, async () => {
      const names = (await fs.readdir(tmp)).sort();
      const pages = await Promise.all(names.map(async n => (await fs.readFile(path.join(tmp, n))).toString('base64')));
      return pages.length;
    });
    console.log(`  ${file}: ${count} pages`);
    if (file.startsWith('Letter') && count > 1) long.push([file, count]);
  }
  await timed(`${phase}/round${round}: list outputs`, () => fs.readdir(path.join(dir, 'output')));
  if (round < 2 && (errors.length || long.length)) {
    for (const [file, value] of errors.length ? errors : long) await agent(`${phase}-repair${round}-${file}`, errors.length ? prompts.fixPrompt(file, value) : prompts.shortenPrompt(file, value), [`cv/${file}`]);
    await render(phase, files, round + 1);
  } else if (errors.length || long.length) throw new Error(`Unresolved document validation: ${JSON.stringify({ errors, long })}`);
}
let server;
try {
  let target = url;
  if (!target) {
    const { createServer } = await import('node:http');
    const posting = { '@context': 'https://schema.org', '@type': 'JobPosting', title: 'Talent Acquisition Specialist', hiringOrganization: { '@type': 'Organization', name: 'Audit Sample Company GmbH' }, jobLocation: { address: { addressLocality: 'Düsseldorf' } }, employmentType: 'FULL_TIME', description: 'Coordinate end-to-end recruiting, interview scheduling, candidate communication and employer branding. Collaborate with hiring managers; perform talent market research. Requirements: recruiting experience, stakeholder management, fluent German and English, strong organizational and communication skills. Hybrid working. Apply to the Hiring Team. No salary or availability specified.' };
    server = createServer((req, res) => { res.setHeader('Content-Type', 'text/html'); res.end(`<html><head><script type="application/ld+json">${JSON.stringify(posting)}</script></head><body><h1>${posting.title}</h1><p>${posting.description}</p></body></html>`); });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    target = `http://127.0.0.1:${server.address().port}/job`;
  }
  const html = await timed('fetch: HTTP + response body', async () => (await fetch(target, { headers: { 'Accept-Language': 'de,en;q=0.8', 'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130 Safari/537.36' }, signal: AbortSignal.timeout(25000) })).text());
  const posting = await timed('fetch: JSON-LD + HTML text extraction', async () => {
    let out = '';
    for (const m of html.matchAll(/<script[^>]*application\/ld\+json[^>]*>(.*?)<\/script>/gis)) if (m[1].includes('JobPosting')) out += `STRUCTURED DATA:\n${m[1].trim()}\n\n`;
    let body = html.replace(/<(script|style|noscript|svg|head)[^>]*>.*?<\/(script|style|noscript|svg|head)>/gis, ' ').replace(/<br\s*\/?>|<\/(p|div|li|h\d)>/gs, '\n').replace(/<[^>]+>/gs, ' ');
    for (const [a, b] of [['&nbsp;', ' '], ['&amp;', '&'], ['&quot;', '"'], ['&#39;', "'"], ['&lt;', '<'], ['&gt;', '>']]) body = body.replaceAll(a, b);
    return out + 'PAGE TEXT:\n' + [...body.replace(/[ \t]+/g, ' ').replace(/\s*\n\s*/g, '\n')].slice(0, 40000).join('');
  });
  await timed('create: input files (SQLite/UI excluded)', async () => {
    await fs.mkdir(path.join(dir, 'input')); await fs.mkdir(path.join(dir, 'output'));
    await fs.writeFile(path.join(dir, 'input/job.txt'), `URL: ${target}\n\n${posting}`);
  });
  await agent('extract', prompts.extractPrompt(), ['job.json']);
  await timed('ingest: JSON parse (SQLite/company matching excluded)', async () => JSON.parse(await fs.readFile(path.join(dir, 'job.json'), 'utf8')));
  await timed('prepare: copy master CV/templates/assets', () => fs.cp(path.join(data, 'cv'), path.join(dir, 'cv'), { recursive: true }));
  await agent('english', prompts.englishPrompt(), ['cv/CV_en.typ', 'cv/Letter_en.typ']);
  await render('english', ['CV_en.typ', 'Letter_en.typ']);
  const englishReadySeconds = (performance.now() - started) / 1000;
  await agent('german', prompts.germanPrompt(), ['cv/CV_de.typ', 'cv/Letter_de.typ']);
  await render('german', ['CV_de.typ', 'Letter_de.typ']);
  await fs.writeFile(path.join(dir, 'milestones.json'), JSON.stringify({ englishReadySeconds }, null, 2));
} catch (e) { warnings.push(`AUDIT FAILED: ${e.message}`); process.exitCode = 1; }
finally {
  server?.close();
  const report = { model, input: url || 'local synthetic HR posting', dir, totalSeconds: (performance.now() - started) / 1000, rows, ai, warnings, exclusions: ['Tauri IPC, Svelte state/paint', 'SQLite insertion/company matching/status updates', 'screenshots', 'live editor preview'], note: 'Standalone replica of backend orchestration using real src/lib/prompts.js; JSON CLI events replace formatted stdout. Event times are observations, not isolated reasoning/network timings.' };
  await fs.writeFile(path.join(dir, 'report.json'), JSON.stringify(report, null, 2));
  console.log(`Report: ${dir}/report.json`);
}
