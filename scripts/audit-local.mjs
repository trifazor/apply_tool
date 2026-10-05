// Supplementary timings for local SQLite work; uses an isolated copy of the DB.
import { DatabaseSync } from 'node:sqlite';
import * as fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { performance } from 'node:perf_hooks';
const dir = path.resolve(process.argv[2]);
const data = path.join(os.homedir(), 'ApplyTool');
const rows = [];
function timed(name, fn) {
  const t = performance.now(); const value = fn();
  const seconds = (performance.now() - t) / 1000;
  rows.push({ name, seconds }); console.log(`${seconds.toFixed(6)}s ${name}`); return value;
}
// Backup through SQLite to include committed WAL data; exclude setup from timings.
const original = new DatabaseSync(path.join(data, 'apply.db'), { readOnly: true });
const tables = original.prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name IN ('jobs','companies')").all();
const companies = original.prepare('SELECT * FROM companies').all();
original.close();
await fs.rm(path.join(dir, 'local-audit.db'), { force: true });
const db = new DatabaseSync(path.join(dir, 'local-audit.db'));
for (const table of tables) db.exec(table.sql);
for (const company of companies) {
  const keys = Object.keys(company);
  db.prepare(`INSERT INTO companies(${keys.join(',')}) VALUES(${keys.map(() => '?').join(',')})`).run(...Object.values(company));
}
const infoRaw = await fs.readFile(path.join(dir, 'job.json'), 'utf8');
const info = JSON.parse(infoRaw);
const id = timed('create: SQLite insert draft', () => db.prepare('INSERT INTO jobs(url) VALUES(?)').run(info.url).lastInsertRowid);
timed('create: SQLite update workdir + load job', () => {
  db.prepare('UPDATE jobs SET workdir=? WHERE id=?').run(dir, id);
  db.prepare('SELECT * FROM jobs WHERE id=?').get(id);
});
const norm = s => s.toLowerCase().replace(/\b(gmbh|mbh|ag|se|kg|kgaa|co|inc|ltd|llc|corp|group|holding|plc|bv|sa|sarl)\b|[^\p{L}\p{N} ]/giu, ' ').trim().split(/\s+/).join(' ');
const match = timed('ingest: company query + normalize/match', () => {
  const n = norm(info.company);
  if (!n) return;
  return db.prepare('SELECT name,note FROM companies').all().find(row => {
    const m = norm(row.name);
    return m && (m === n || ` ${n} `.includes(` ${m} `) || ` ${m} `.includes(` ${n} `));
  });
});
timed('ingest: SQLite update extracted info + load', () => {
  db.prepare('UPDATE jobs SET title=?,company=?,location=?,info=?,flagged=?,flag_reason=? WHERE id=?').run(info.title, info.company, info.location, infoRaw, match ? 1 : 0, match?.name ?? '', id);
  db.prepare('SELECT * FROM jobs WHERE id=?').get(id);
});
timed('refresh: list jobs + companies', () => { db.prepare('SELECT * FROM jobs ORDER BY id DESC').all(); db.prepare('SELECT * FROM companies ORDER BY name').all(); });
timed('finish: draft to ready + load', () => { db.prepare("UPDATE jobs SET status='ready' WHERE id=? AND status='draft'").run(id); db.prepare('SELECT * FROM jobs WHERE id=?').get(id); });
db.close();
await fs.writeFile(path.join(dir, 'local-report.json'), JSON.stringify({ rows, companyCount: companies.length, flagged: !!match, note: 'Separate supplementary run, isolated SQLite database, equivalent SQL and JS company normalization; not Rust regex/IPC timing.' }, null, 2));
