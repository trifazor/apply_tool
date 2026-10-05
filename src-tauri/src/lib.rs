mod preview;

use base64::Engine;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use tauri::{Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};

struct Db(Mutex<Connection>);
type R<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Everything lives in ~/ApplyTool so host CLIs (outside the flatpak) see the same paths.
fn root() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join("ApplyTool")
}
fn skills_dir() -> PathBuf { root().join("skills") }
fn cv_dir() -> PathBuf { root().join("cv") }

fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Bundled tools (typst, pandoc) live in /app/bin inside the flatpak; otherwise use PATH.
fn tool(name: &str) -> tokio::process::Command {
    let bundled = Path::new("/app/bin").join(name);
    let mut c = tokio::process::Command::new(if bundled.exists() { bundled } else { PathBuf::from(name) });
    c.stdin(Stdio::null());
    c
}

/// Build a command that runs `script` in a login shell on the host (escaping the flatpak sandbox).
fn host_cmd(script: &str, cwd: &Path) -> tokio::process::Command {
    let mut c;
    if in_flatpak() {
        c = tokio::process::Command::new("flatpak-spawn");
        c.arg("--host")
            .arg(format!("--directory={}", cwd.display()))
            .args(["bash", "-lc", script]);
    } else {
        c = tokio::process::Command::new("bash");
        c.args(["-lc", script]).current_dir(cwd);
    }
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}

const DEFAULTS: &[(&str, &str)] = &[
    ("agent", "claude"),
    // $MODEL is set from Settings (empty = the CLI's own default model)
    // Only the Read tool (screenshots/PDFs) and no MCP servers: context is inlined, files come back as text blocks.
    ("cmd_claude", r#"claude ${MODEL:+--model "$MODEL"} --tools Read --strict-mcp-config -p "$(cat .prompt.md)" --dangerously-skip-permissions"#),
    ("cmd_claude_cont", r#"claude ${MODEL:+--model "$MODEL"} --tools Read --strict-mcp-config -c -p "$(cat .prompt.md)" --dangerously-skip-permissions"#),
    ("cmd_codex", r#"codex exec --skip-git-repo-check --full-auto ${MODEL:+-m "$MODEL"} "$(cat .prompt.md)""#),
    ("cmd_codex_cont", r#"codex exec --skip-git-repo-check --full-auto ${MODEL:+-m "$MODEL"} resume --last "$(cat .prompt.md)""#),
    ("cmd_opencode", r#"opencode run ${MODEL:+-m "$MODEL"} "$(cat .prompt.md)""#),
    ("cmd_opencode_cont", r#"opencode run ${MODEL:+-m "$MODEL"} -c "$(cat .prompt.md)""#),
    ("model_claude", ""),
    ("model_codex", ""),
    ("model_opencode", ""),
];

const PREV_DEFAULTS: &[(&str, &str)] = &[
    ("cmd_claude", r#"claude ${MODEL:+--model "$MODEL"} -p "$(cat .prompt.md)" --dangerously-skip-permissions"#),
    ("cmd_claude_cont", r#"claude ${MODEL:+--model "$MODEL"} -c -p "$(cat .prompt.md)" --dangerously-skip-permissions"#),
];

const DEFAULT_SKILLS: &[(&str, &str)] = &[
    ("01-general.md", include_str!("../defaults/skills/01-general.md")),
    ("02-cv.md", include_str!("../defaults/skills/02-cv.md")),
    ("03-letter.md", include_str!("../defaults/skills/03-letter.md")),
    ("04-typst.md", include_str!("../defaults/skills/04-typst.md")),
];
const DEFAULT_CV: &[(&str, &str)] = &[
    ("master.typ", include_str!("../defaults/master.typ")),
    ("letter.typ", include_str!("../defaults/letter.typ")),
];

fn init_db() -> Connection {
    for d in [root().join("jobs"), root().join("fonts"), root().join("applications"), skills_dir(), cv_dir()] {
        std::fs::create_dir_all(d).ok();
    }
    if std::fs::read_dir(skills_dir()).map(|mut d| d.next().is_none()).unwrap_or(true) {
        for (n, s) in DEFAULT_SKILLS { std::fs::write(skills_dir().join(n), s).ok(); }
    }
    for (n, s) in DEFAULT_CV {
        if !cv_dir().join(n).exists() { std::fs::write(cv_dir().join(n), s).ok(); }
    }
    let c = Connection::open(root().join("apply.db")).expect("open db");
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS companies(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE, note TEXT NOT NULL DEFAULT '');
         CREATE TABLE IF NOT EXISTS jobs(
           id INTEGER PRIMARY KEY, created_at TEXT NOT NULL DEFAULT (datetime('now')),
           title TEXT NOT NULL DEFAULT '', company TEXT NOT NULL DEFAULT '', location TEXT NOT NULL DEFAULT '',
           url TEXT NOT NULL DEFAULT '', info TEXT NOT NULL DEFAULT '{}',
           flagged INTEGER NOT NULL DEFAULT 0, flag_reason TEXT NOT NULL DEFAULT '',
           status TEXT NOT NULL DEFAULT 'draft', archived INTEGER NOT NULL DEFAULT 0,
           applied_at TEXT, notes TEXT NOT NULL DEFAULT '', workdir TEXT NOT NULL DEFAULT '',
           chat TEXT NOT NULL DEFAULT '[]');",
    )
    .expect("schema");
    // migrations (errors = already applied)
    c.execute("ALTER TABLE jobs ADD COLUMN snapshot TEXT NOT NULL DEFAULT ''", []).ok();
    for (k, v) in DEFAULTS {
        c.execute("INSERT OR IGNORE INTO settings(key,value) VALUES(?1,?2)", params![k, v]).ok();
        // agent commands from before model selection existed
        if k.starts_with("cmd_") {
            c.execute("UPDATE settings SET value=?2 WHERE key=?1 AND value NOT LIKE '%MODEL%'", params![k, v]).ok();
        }
    }
    // unchanged previous defaults → current defaults
    for (k, old) in PREV_DEFAULTS {
        if let Some((_, v)) = DEFAULTS.iter().find(|(d, _)| d == k) {
            c.execute("UPDATE settings SET value=?2 WHERE key=?1 AND value=?3", params![k, v, old]).ok();
        }
    }
    c
}

fn setting(c: &Connection, k: &str) -> String {
    c.query_row("SELECT value FROM settings WHERE key=?1", [k], |r| r.get(0)).unwrap_or_default()
}

fn job_dir(c: &Connection, id: i64) -> R<PathBuf> {
    c.query_row("SELECT workdir FROM jobs WHERE id=?1", [id], |r| r.get::<_, String>(0))
        .map(PathBuf::from).map_err(err)
}

/// Copy every file of `src` into `dst` (non-recursive is enough for cv assets).
fn copy_dir(src: &Path, dst: &Path) -> R<()> {
    std::fs::create_dir_all(dst).map_err(err)?;
    for e in std::fs::read_dir(src).map_err(err)?.flatten() {
        if e.path().is_file() { std::fs::copy(e.path(), dst.join(e.file_name())).map_err(err)?; }
    }
    Ok(())
}

// ---------- settings ----------
#[tauri::command]
fn get_settings(db: State<Db>) -> R<Value> {
    let c = db.0.lock().unwrap();
    let mut s = c.prepare("SELECT key,value FROM settings").map_err(err)?;
    let mut m = serde_json::Map::new();
    for row in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(err)? {
        let (k, v) = row.map_err(err)?;
        m.insert(k, Value::String(v));
    }
    m.insert("root".into(), Value::String(root().display().to_string()));
    Ok(Value::Object(m))
}

#[tauri::command]
fn set_setting(db: State<Db>, key: String, value: String) -> R<()> {
    db.0.lock().unwrap()
        .execute("INSERT OR REPLACE INTO settings(key,value) VALUES(?1,?2)", params![key, value])
        .map(|_| ()).map_err(err)
}

#[tauri::command]
fn reset_setting(db: State<Db>, key: String) -> R<String> {
    let v = DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).ok_or("no default")?;
    set_setting(db, key, v.into())?;
    Ok(v.into())
}

// ---------- skills (markdown files the AI follows) ----------
#[derive(Serialize)]
struct Skill { name: String, content: String }

fn safe_name(name: &str, ext: &str) -> R<String> {
    let n: String = name.trim().chars().map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '-' }).collect();
    let n = n.trim_matches('.').to_string();
    if n.is_empty() { return Err("empty name".into()); }
    Ok(if n.ends_with(ext) { n } else { format!("{n}{ext}") })
}

#[tauri::command]
fn list_skills() -> R<Vec<Skill>> {
    let mut v: Vec<Skill> = std::fs::read_dir(skills_dir()).map_err(err)?.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .map(|e| Skill { name: e.file_name().to_string_lossy().into(), content: std::fs::read_to_string(e.path()).unwrap_or_default() })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(v)
}

#[tauri::command]
fn save_skill(name: String, content: String) -> R<String> {
    let n = safe_name(&name, ".md")?;
    std::fs::write(skills_dir().join(&n), content).map_err(err)?;
    Ok(n)
}

#[tauri::command]
fn delete_skill(name: String) -> R<()> {
    std::fs::remove_file(skills_dir().join(safe_name(&name, ".md")?)).map_err(err)
}

#[tauri::command]
fn restore_default_skills() -> R<()> {
    for (n, s) in DEFAULT_SKILLS { std::fs::write(skills_dir().join(n), s).map_err(err)?; }
    Ok(())
}

/// All skills concatenated into SKILLS.md inside the agent's working dir.
fn write_skills(dir: &Path) -> R<()> {
    let body = list_skills()?.into_iter().map(|s| s.content.trim().to_string()).collect::<Vec<_>>().join("\n\n---\n\n");
    std::fs::write(dir.join("SKILLS.md"), format!("# SKILLS — follow ALL of these rules\n\n{body}\n")).map_err(err)
}

// ---------- master CV / letter templates ----------
#[tauri::command]
fn read_cv_file(name: String) -> R<String> {
    std::fs::read_to_string(cv_dir().join(safe_name(&name, ".typ")?)).map_err(err)
}

#[tauri::command]
fn write_cv_file(name: String, content: String) -> R<()> {
    std::fs::write(cv_dir().join(safe_name(&name, ".typ")?), content).map_err(err)
}

#[tauri::command]
fn restore_default_cv_file(name: String) -> R<String> {
    let (_, s) = DEFAULT_CV.iter().find(|(n, _)| *n == name).ok_or("unknown template")?;
    std::fs::write(cv_dir().join(&name), s).map_err(err)?;
    Ok(s.to_string())
}

/// Copy an existing CV (docx/odt/pdf/…) into the cv dir; docx/odt also get a markdown version for the AI.
#[tauri::command]
async fn import_cv_source(path: String) -> R<String> {
    let src = Path::new(&path);
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    for e in std::fs::read_dir(cv_dir()).map_err(err)?.flatten() {
        if e.file_name().to_string_lossy().starts_with("source.") { std::fs::remove_file(e.path()).ok(); }
    }
    let dst = cv_dir().join(format!("source.{ext}"));
    std::fs::copy(src, &dst).map_err(err)?;
    if ["docx", "odt", "rtf", "html", "md"].contains(&ext.as_str()) {
        let out = tool("pandoc").current_dir(cv_dir())
            .args([&format!("source.{ext}"), "-t", "markdown", "-o", "source.md", "--extract-media=."])
            .output().await.map_err(|e| format!("pandoc not available: {e}"))?;
        if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).into()); }
    }
    Ok(dst.display().to_string())
}

// ---------- typst / pandoc ----------
/// Render a .typ file to PNG pages (base64) for in-app preview.
async fn render_pages(file: &Path, ppi: u32) -> R<Vec<String>> {
    let tmp = std::env::temp_dir().join(format!("applytool-{}-{}", std::process::id(), file.file_stem().unwrap_or_default().to_string_lossy()));
    std::fs::remove_dir_all(&tmp).ok();
    std::fs::create_dir_all(&tmp).map_err(err)?;
    let out = tool("typst").kill_on_drop(true)
        .args(["compile", "--root", "/", "--font-path"]).arg(root().join("fonts"))
        .args(["--format", "png", "--ppi", &ppi.to_string()]).arg(file).arg(tmp.join("p{0p}.png"))
        .output().await.map_err(|e| format!("typst not available: {e}"))?;
    if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).into()); }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&tmp).map_err(err)?.flatten().map(|e| e.path()).collect();
    files.sort();
    let pages = files.iter().filter_map(|p| std::fs::read(p).ok())
        .map(|b| base64::engine::general_purpose::STANDARD.encode(b)).collect();
    std::fs::remove_dir_all(&tmp).ok();
    Ok(pages)
}

#[tauri::command]
async fn preview_cv_file(name: String) -> R<Vec<String>> {
    render_pages(&cv_dir().join(safe_name(&name, ".typ")?), 70).await
}

fn company_slug(company: &str) -> String {
    let s: String = company.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    let s = s.split('_').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("_");
    if s.is_empty() { "Company".into() } else { s }
}

/// Compile job/cv/CV_de.typ, CV_en.typ and Letter.typ into PDF + DOCX + ODT in job/output, plus page previews.
/// `only` limits it to some documents (keys "cv_en", "cv_de", "letter"), e.g. to show the English CV early.
#[tauri::command]
async fn compile_job(db: State<'_, Db>, id: i64, only: Option<Vec<String>>) -> R<Value> {
    let (dir, company) = {
        let c = db.0.lock().unwrap();
        let company: String = c.query_row("SELECT company FROM jobs WHERE id=?1", [id], |r| r.get(0)).map_err(err)?;
        (job_dir(&c, id)?, company)
    };
    let slug = company_slug(&company);
    let (src, out) = (dir.join("cv"), dir.join("output"));
    std::fs::create_dir_all(&out).map_err(err)?;
    let mut warnings = vec![];
    let mut previews = serde_json::Map::new();
    let mut errors = serde_json::Map::new();
    let docs = [
        ("CV_en.typ", format!("CV_EN_{slug}"), "cv_en"),
        ("Letter_en.typ", format!("Motivation_Letter_EN_{slug}"), "letter_en"),
        ("CV_de.typ", format!("CV_DE_{slug}"), "cv_de"),
        ("Letter_de.typ", format!("Motivation_Letter_DE_{slug}"), "letter_de"),
    ];
    for (file, base, key) in docs {
        if only.as_ref().is_some_and(|o| !o.iter().any(|k| k == key)) { continue; }
        // drop stale outputs of this document (e.g. from a different company name)
        let prefix = base.trim_end_matches(&slug).to_string();
        for e in std::fs::read_dir(&out).map_err(err)?.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) { std::fs::remove_file(e.path()).ok(); }
        }
        let f = src.join(file);
        if !f.exists() { warnings.push(format!("{file} was not created by the AI")); continue; }
        let r = tool("typst").args(["compile", "--root"]).arg(&dir).arg("--font-path").arg(root().join("fonts"))
            .arg(&f).arg(out.join(format!("{base}.pdf"))).output().await.map_err(|e| format!("typst: {e}"))?;
        if !r.status.success() {
            let e = String::from_utf8_lossy(&r.stderr).to_string();
            warnings.push(format!("{file}: {e}"));
            errors.insert(file.into(), json!(e));
            continue;
        }
        for ext in ["docx", "odt"] {
            let r = tool("pandoc").current_dir(&src).args(["-f", "typst", file, "-o"]).arg(out.join(format!("{base}.{ext}")))
                .output().await.map_err(|e| format!("pandoc: {e}"))?;
            if !r.status.success() { warnings.push(format!("{file} → {ext}: {}", String::from_utf8_lossy(&r.stderr))); }
        }
        match render_pages(&f, 70).await {
            Ok(p) => {
                if key.starts_with("letter") && p.len() > 1 { warnings.push(format!("LETTER_TOO_LONG:{file}:{}", p.len())); }
                previews.insert(key.into(), json!(p));
            }
            Err(e) => warnings.push(e),
        }
    }
    let files = list_outputs(db, id)?;
    Ok(json!({ "files": files, "previews": previews, "warnings": warnings, "errors": errors }))
}

// ---------- companies ----------
#[derive(Serialize)]
struct Company { id: i64, name: String, note: String }

#[tauri::command]
fn list_companies(db: State<Db>) -> R<Vec<Company>> {
    let c = db.0.lock().unwrap();
    let mut s = c.prepare("SELECT id,name,note FROM companies ORDER BY name COLLATE NOCASE").map_err(err)?;
    let v = s.query_map([], |r| Ok(Company { id: r.get(0)?, name: r.get(1)?, note: r.get(2)? }))
        .map_err(err)?.filter_map(Result::ok).collect();
    Ok(v)
}

fn insert_companies(c: &Connection, rows: impl Iterator<Item = (String, String)>) -> R<usize> {
    let mut n = 0;
    for (name, note) in rows {
        let name = name.trim();
        if name.is_empty() { continue; }
        n += c.execute("INSERT OR IGNORE INTO companies(name,note) VALUES(?1,?2)", params![name, note.trim()]).map_err(err)?;
    }
    Ok(n)
}

/// One company per line, optional note after `;` or a tab.
#[tauri::command]
fn add_companies(db: State<Db>, text: String) -> R<usize> {
    let rows = text.lines().map(|l| {
        let (a, b) = l.split_once([';', '\t']).unwrap_or((l, ""));
        (a.to_string(), b.to_string())
    }).collect::<Vec<_>>();
    insert_companies(&db.0.lock().unwrap(), rows.into_iter())
}

/// Minimal CSV parser: auto-detects `,` `;` or tab, handles quotes, skips a header row.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let first = text.lines().next().unwrap_or("");
    let delim = [';', '\t', ','].into_iter().max_by_key(|d| first.matches(*d).count()).unwrap_or(',');
    let (mut rows, mut row, mut cell, mut quoted) = (vec![], vec![], String::new(), false);
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => { cell.push('"'); chars.next(); }
            '"' => quoted = !quoted,
            c if c == delim && !quoted => row.push(std::mem::take(&mut cell)),
            '\n' if !quoted => { row.push(std::mem::take(&mut cell)); rows.push(std::mem::take(&mut row)); }
            '\r' if !quoted => {}
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() { row.push(cell); rows.push(row); }
    rows
}

#[tauri::command]
fn import_companies_csv(db: State<Db>, path: String) -> R<usize> {
    let bytes = std::fs::read(&path).map_err(err)?;
    let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect()); // latin-1 fallback
    let mut rows = parse_csv(&text);
    // pick the "name"-like column from the header if there is one
    let mut col = 0;
    if let Some(h) = rows.first() {
        let lower: Vec<String> = h.iter().map(|s| s.trim().to_lowercase()).collect();
        if let Some(i) = lower.iter().position(|s| ["company", "name", "company name", "firma", "unternehmen", "arbeitgeber", "employer"].contains(&s.as_str())) {
            col = i;
            rows.remove(0);
        }
    }
    let it = rows.into_iter().map(|r| {
        let name = r.get(col).cloned().unwrap_or_default();
        let note = r.iter().enumerate().filter(|(i, s)| *i != col && !s.trim().is_empty()).map(|(_, s)| s.trim()).collect::<Vec<_>>().join(" · ");
        (name, note)
    });
    insert_companies(&db.0.lock().unwrap(), it)
}

#[tauri::command]
fn delete_company(db: State<Db>, id: i64) -> R<()> {
    db.0.lock().unwrap().execute("DELETE FROM companies WHERE id=?1", [id]).map(|_| ()).map_err(err)
}

#[tauri::command]
fn clear_companies(db: State<Db>) -> R<()> {
    db.0.lock().unwrap().execute("DELETE FROM companies", []).map(|_| ()).map_err(err)
}

fn norm(s: &str) -> String {
    let re = regex::Regex::new(r"(?i)\b(gmbh|mbh|ag|se|kg|kgaa|co|inc|ltd|llc|corp|group|holding|plc|bv|sa|sarl)\b|[^\p{L}\p{N} ]").unwrap();
    re.replace_all(&s.to_lowercase(), " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn match_company(c: &Connection, company: &str) -> Option<(String, String)> {
    let n = norm(company);
    if n.is_empty() { return None; }
    let mut s = c.prepare("SELECT name,note FROM companies").ok()?;
    let rows: Vec<(String, String)> = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).ok()?.filter_map(Result::ok).collect();
    rows.into_iter().find(|(name, _)| {
        let m = norm(name);
        !m.is_empty() && (m == n || format!(" {n} ").contains(&format!(" {m} ")) || format!(" {m} ").contains(&format!(" {n} ")))
    })
}

// ---------- fetching ----------
#[tauri::command]
async fn fetch_url(url: String) -> R<String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130 Safari/537.36")
        .timeout(std::time::Duration::from_secs(25))
        .build().map_err(err)?;
    let html = client.get(&url).header("Accept-Language", "de,en;q=0.8").send().await.map_err(err)?
        .text().await.map_err(err)?;
    // Structured JobPosting data (LinkedIn, StepStone, XING, Indeed… usually ship it)
    let ld = regex::Regex::new(r#"(?is)<script[^>]*application/ld\+json[^>]*>(.*?)</script>"#).unwrap();
    let mut out = String::new();
    for cap in ld.captures_iter(&html) {
        if cap[1].contains("JobPosting") { out.push_str("STRUCTURED DATA:\n"); out.push_str(cap[1].trim()); out.push_str("\n\n"); }
    }
    let strip = regex::Regex::new(r"(?is)<(script|style|noscript|svg|head)[^>]*>.*?</(script|style|noscript|svg|head)>").unwrap();
    let body = strip.replace_all(&html, " ");
    let body = regex::Regex::new(r"(?s)<br\s*/?>|</(p|div|li|h\d)>").unwrap().replace_all(&body, "\n");
    let body = regex::Regex::new(r"(?s)<[^>]+>").unwrap().replace_all(&body, " ");
    let body = body.replace("&nbsp;", " ").replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">");
    let body = regex::Regex::new(r"[ \t]+").unwrap().replace_all(&body, " ");
    let body = regex::Regex::new(r"\s*\n\s*").unwrap().replace_all(&body, "\n");
    out.push_str("PAGE TEXT:\n");
    out.push_str(&body.chars().take(40_000).collect::<String>());
    Ok(out)
}

// ---------- jobs ----------
const JOB_COLS: &str = "id,created_at,title,company,location,url,info,flagged,flag_reason,status,archived,applied_at,notes,workdir,chat,snapshot";

fn job_row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?, "created_at": r.get::<_, String>(1)?, "title": r.get::<_, String>(2)?,
        "company": r.get::<_, String>(3)?, "location": r.get::<_, String>(4)?, "url": r.get::<_, String>(5)?,
        "info": serde_json::from_str::<Value>(&r.get::<_, String>(6)?).unwrap_or(Value::Null),
        "flagged": r.get::<_, i64>(7)? != 0, "flag_reason": r.get::<_, String>(8)?, "status": r.get::<_, String>(9)?,
        "archived": r.get::<_, i64>(10)? != 0, "applied_at": r.get::<_, Option<String>>(11)?, "notes": r.get::<_, String>(12)?,
        "workdir": r.get::<_, String>(13)?,
        "chat": serde_json::from_str::<Value>(&r.get::<_, String>(14)?).unwrap_or(json!([])),
        "snapshot": r.get::<_, String>(15)?,
    }))
}

fn load_job(c: &Connection, id: i64) -> R<Value> {
    c.query_row(&format!("SELECT {JOB_COLS} FROM jobs WHERE id=?1"), [id], job_row).map_err(err)
}

#[tauri::command]
fn list_jobs(db: State<Db>) -> R<Vec<Value>> {
    let c = db.0.lock().unwrap();
    let mut s = c.prepare(&format!("SELECT {JOB_COLS} FROM jobs ORDER BY id DESC")).map_err(err)?;
    let v = s.query_map([], job_row).map_err(err)?.filter_map(Result::ok).collect();
    Ok(v)
}

#[tauri::command]
fn get_job(db: State<Db>, id: i64) -> R<Value> {
    load_job(&db.0.lock().unwrap(), id)
}

#[tauri::command]
fn create_job(db: State<Db>, text: String, url: String, screenshots: Vec<String>) -> R<Value> {
    let c = db.0.lock().unwrap();
    c.execute("INSERT INTO jobs(url) VALUES(?1)", [&url]).map_err(err)?;
    let id = c.last_insert_rowid();
    let dir = root().join("jobs").join(format!("{id:04}"));
    std::fs::create_dir_all(dir.join("input")).map_err(err)?;
    std::fs::write(dir.join("input/job.txt"), format!("URL: {url}\n\n{text}")).map_err(err)?;
    for (i, s) in screenshots.iter().enumerate() {
        let p = Path::new(s);
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("png");
        std::fs::copy(p, dir.join(format!("input/screenshot_{}.{ext}", i + 1))).map_err(err)?;
    }
    c.execute("UPDATE jobs SET workdir=?1 WHERE id=?2", params![dir.display().to_string(), id]).map_err(err)?;
    load_job(&c, id)
}

/// Reads job.json written by the agent, stores it and runs the company check.
#[tauri::command]
fn ingest_job_info(db: State<Db>, id: i64) -> R<Value> {
    let c = db.0.lock().unwrap();
    let dir = job_dir(&c, id)?;
    let raw = std::fs::read_to_string(dir.join("job.json")).map_err(|e| format!("The AI did not write job.json: {e}"))?;
    let info: Value = serde_json::from_str(&raw).map_err(|e| format!("job.json is not valid JSON: {e}"))?;
    let g = |k: &str| info.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let company = g("company");
    let hit = match_company(&c, &company);
    let reason = hit.map(|(n, note)| if note.is_empty() { format!("Matches \"{n}\" in your company list") } else { format!("Matches \"{n}\": {note}") }).unwrap_or_default();
    c.execute("UPDATE jobs SET title=?1, company=?2, location=?3, info=?4, flagged=?5, flag_reason=?6 WHERE id=?7",
        params![g("title"), company, g("location"), raw, !reason.is_empty() as i64, reason, id]).map_err(err)?;
    load_job(&c, id)
}

#[tauri::command]
fn update_job(db: State<Db>, id: i64, status: String, notes: String, archived: bool) -> R<Value> {
    let c = db.0.lock().unwrap();
    c.execute("UPDATE jobs SET status=?1, notes=?2, archived=?3,
               applied_at = CASE WHEN ?1 NOT IN ('draft','ready') AND applied_at IS NULL THEN datetime('now') ELSE applied_at END WHERE id=?4",
        params![status, notes, archived as i64, id]).map_err(err)?;
    let job = load_job(&c, id)?;
    // First time the job counts as applied: freeze what was sent.
    if !["draft", "ready"].contains(&status.as_str()) && job["snapshot"].as_str().unwrap_or("").is_empty() {
        let dir = write_snapshot(&job)?;
        c.execute("UPDATE jobs SET snapshot=?1 WHERE id=?2", params![dir.display().to_string(), id]).map_err(err)?;
        return load_job(&c, id);
    }
    Ok(job)
}

/// Copy the documents, posting and notes into ~/ApplyTool/applications/<date>_<company>_<title>_<id>/.
fn write_snapshot(job: &Value) -> R<PathBuf> {
    let g = |k: &str| job[k].as_str().unwrap_or("").to_string();
    let applied = g("applied_at");
    let title: String = company_slug(&g("title")).chars().take(40).collect();
    let dir = root().join("applications").join(format!("{}_{}_{}_{}", applied.get(..10).unwrap_or("undated"), company_slug(&g("company")), title, job["id"]));
    let work = PathBuf::from(g("workdir"));
    if work.join("output").exists() { copy_dir(&work.join("output"), &dir.join("documents"))?; }
    std::fs::create_dir_all(dir.join("documents/source")).map_err(err)?;
    for e in std::fs::read_dir(work.join("cv")).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if n.ends_with(".typ") && n != "master.typ" && n != "letter.typ" { std::fs::copy(e.path(), dir.join("documents/source").join(&n)).ok(); }
    }
    copy_dir(&work.join("input"), &dir.join("posting"))?;
    if work.join("job.json").exists() { std::fs::copy(work.join("job.json"), dir.join("posting/job.json")).map_err(err)?; }
    let info = &job["info"];
    let list = |k: &str| info[k].as_array().map(|a| a.iter().filter_map(Value::as_str).map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n")).unwrap_or_default();
    let md = format!(
        "# {title}\n\n**Company:** {company}  \n**Location:** {loc}  \n**Applied:** {applied}  \n**Status at application:** {status}  \n**Posting:** {url}\n\n## Notes at the time of application\n\n{notes}\n\n## Job description\n\n{summary}\n\n### Responsibilities\n{resp}\n\n### Requirements\n{req}\n",
        title = g("title"), company = g("company"), loc = g("location"), status = g("status"), url = g("url"),
        notes = if g("notes").trim().is_empty() { "_(none)_".into() } else { g("notes") },
        summary = info["summary"].as_str().unwrap_or(""), resp = list("responsibilities"), req = list("requirements"));
    std::fs::write(dir.join("APPLICATION.md"), md).map_err(err)?;
    Ok(dir)
}

/// Re-create the snapshot from the current documents and notes (e.g. after sending an updated CV).
#[tauri::command]
fn refresh_snapshot(db: State<Db>, id: i64) -> R<Value> {
    let c = db.0.lock().unwrap();
    let job = load_job(&c, id)?;
    if let Some(old) = job["snapshot"].as_str().filter(|s| !s.is_empty()) { std::fs::remove_dir_all(old).ok(); }
    let dir = write_snapshot(&job)?;
    c.execute("UPDATE jobs SET snapshot=?1 WHERE id=?2", params![dir.display().to_string(), id]).map_err(err)?;
    load_job(&c, id)
}

/// Files + frozen summary of the application snapshot.
#[tauri::command]
fn get_snapshot(db: State<Db>, id: i64) -> R<Value> {
    let snap: String = db.0.lock().unwrap().query_row("SELECT snapshot FROM jobs WHERE id=?1", [id], |r| r.get(0)).map_err(err)?;
    if snap.is_empty() || !Path::new(&snap).exists() { return Ok(Value::Null); }
    let mut files: Vec<String> = std::fs::read_dir(Path::new(&snap).join("documents")).map_err(err)?.flatten()
        .filter(|e| e.path().is_file()).map(|e| e.path().display().to_string()).collect();
    files.sort();
    let md = std::fs::read_to_string(Path::new(&snap).join("APPLICATION.md")).unwrap_or_default();
    Ok(json!({ "dir": snap, "files": files, "summary": md }))
}

#[tauri::command]
fn delete_job(db: State<Db>, id: i64, keep_snapshot: bool) -> R<()> {
    let c = db.0.lock().unwrap();
    if let Ok(dir) = job_dir(&c, id) {
        if dir.starts_with(root().join("jobs")) { std::fs::remove_dir_all(dir).ok(); }
    }
    if !keep_snapshot {
        let snap: String = c.query_row("SELECT snapshot FROM jobs WHERE id=?1", [id], |r| r.get(0)).unwrap_or_default();
        if !snap.is_empty() && Path::new(&snap).starts_with(root().join("applications")) { std::fs::remove_dir_all(snap).ok(); }
    }
    c.execute("DELETE FROM jobs WHERE id=?1", [id]).map(|_| ()).map_err(err)
}

#[tauri::command]
fn clear_chat(db: State<Db>, id: i64) -> R<Value> {
    let c = db.0.lock().unwrap();
    c.execute("UPDATE jobs SET chat='[]' WHERE id=?1", [id]).map_err(err)?;
    load_job(&c, id)
}

/// Models offered by the selected CLI.
#[tauri::command]
async fn list_models(agent: String) -> R<Vec<String>> {
    let script = match agent.as_str() {
        // Claude Code has no list command: documented aliases + current model ids
        "claude" => return Ok(["fable", "opus", "sonnet", "haiku", "claude-fable-5-1", "claude-opus-5-5", "claude-sonnet-5", "claude-haiku-4-5"].map(String::from).to_vec()),
        "codex" => "codex debug models",
        "opencode" => "opencode models opencode-go",
        _ => return Err("unknown agent".into()),
    };
    let out = host_cmd(script, &root()).output().await.map_err(err)?;
    if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).into()); }
    let text = String::from_utf8_lossy(&out.stdout);
    if agent == "codex" {
        let v: Value = serde_json::from_str(&text).map_err(err)?;
        return Ok(v["models"].as_array().into_iter().flatten()
            .filter(|m| m["visibility"].as_str().unwrap_or("list") == "list")
            .filter_map(|m| m["slug"].as_str().map(String::from)).collect());
    }
    Ok(text.lines().map(str::trim).filter(|l| l.contains('/')).map(String::from).collect())
}

fn job_cv_file(c: &Connection, id: i64, name: &str) -> R<PathBuf> {
    let ok = regex::Regex::new(r"^(CV|Letter)_(en|de)\.typ$").unwrap();
    if !ok.is_match(name) { return Err(format!("not an editable document: {name}")); }
    Ok(job_dir(c, id)?.join("cv").join(name))
}

#[tauri::command]
fn read_job_file(db: State<Db>, id: i64, name: String) -> R<String> {
    std::fs::read_to_string(job_cv_file(&db.0.lock().unwrap(), id, &name)?).map_err(err)
}

#[tauri::command]
fn write_job_file(db: State<Db>, id: i64, name: String, content: String) -> R<()> {
    std::fs::write(job_cv_file(&db.0.lock().unwrap(), id, &name)?, content).map_err(err)
}

/// Render unsaved editor text with the bundled CLI, without overwriting the document.
/// Keep the temporary source beside the original so relative images/includes still work.
#[tauri::command]
async fn preview_live(db: State<'_, Db>, id: i64, name: String, content: String, on_preview: tauri::ipc::Channel<Value>) -> R<()> {
    let file = job_cv_file(&db.0.lock().unwrap(), id, &name)?;
    let result = render_editor_preview(&file, &content).await?;
    // Images travel through Tauri's channel transport. A large asynchronous
    // custom-protocol response can stall in the Linux WebKit webview.
    on_preview.send(result).map_err(err)
}

pub async fn render_editor_preview(file: &Path, content: &str) -> R<Value> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(err)?.as_nanos();
    let temporary = file.with_file_name(format!(".editor-{}-{stamp}.typ", std::process::id()));
    std::fs::write(&temporary, content).map_err(err)?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(8), render_pages(&temporary, 70))
        .await.unwrap_or_else(|_| Err("Typst preview exceeded 8 seconds".into()));
    std::fs::remove_file(&temporary).ok();
    // Also clean up PNGs when a timeout cancels render_pages before its own cleanup.
    let tmp = std::env::temp_dir().join(format!("applytool-{}-{}", std::process::id(), temporary.file_stem().unwrap_or_default().to_string_lossy()));
    std::fs::remove_dir_all(tmp).ok();
    match result {
        Ok(pages) => {
            let changed: Vec<Value> = pages.iter().enumerate().map(|(i, png)| json!([i, png])).collect();
            Ok(json!({ "count": pages.len(), "changed": changed, "error": null }))
        }
        Err(e) => Ok(json!({ "count": 0, "changed": [], "error": e })),
    }
}

#[tauri::command]
fn preview_close() { preview::reset(); }

/// The original posting text (pasted text and fetched page) for reference.
#[tauri::command]
fn get_posting(db: State<Db>, id: i64) -> R<String> {
    let dir = job_dir(&db.0.lock().unwrap(), id)?;
    Ok(std::fs::read_to_string(dir.join("input/job.txt")).unwrap_or_default())
}

#[tauri::command]
fn list_outputs(db: State<Db>, id: i64) -> R<Vec<String>> {
    let dir = job_dir(&db.0.lock().unwrap(), id)?;
    let mut v: Vec<String> = std::fs::read_dir(dir.join("output")).map(|rd| {
        rd.filter_map(Result::ok).map(|e| e.path().display().to_string()).collect()
    }).unwrap_or_default();
    v.sort();
    Ok(v)
}

/// Fresh copy of the master CV + letter template (and assets) into the job before generating.
#[tauri::command]
fn prepare_job_cv(db: State<Db>, id: i64) -> R<()> {
    let dir = job_dir(&db.0.lock().unwrap(), id)?;
    copy_dir(&cv_dir(), &dir.join("cv"))
}

fn push_chat(c: &Connection, id: i64, role: &str, text: &str) {
    let raw: String = c.query_row("SELECT chat FROM jobs WHERE id=?1", [id], |r| r.get(0)).unwrap_or("[]".into());
    let mut chat: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();
    chat.push(json!({ "role": role, "text": text, "at": chrono_now(c) }));
    c.execute("UPDATE jobs SET chat=?1 WHERE id=?2", params![Value::Array(chat).to_string(), id]).ok();
}

fn chrono_now(c: &Connection) -> String {
    c.query_row("SELECT datetime('now','localtime')", [], |r| r.get(0)).unwrap_or_default()
}

/// Run a script on the host in `dir`, streaming output lines as `job-log` events.
async fn run_streaming(app: &tauri::AppHandle, id: i64, script: &str, dir: &Path) -> R<(bool, String)> {
    let mut child = host_cmd(script, dir).spawn().map_err(|e| format!("spawn failed: {e}"))?;
    let (so, se) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let a2 = app.clone();
    let errs = tokio::spawn(async move {
        let mut l = BufReader::new(se).lines();
        let mut buf = String::new();
        while let Ok(Some(line)) = l.next_line().await {
            a2.emit("job-log", json!({ "id": id, "line": line })).ok();
            buf.push_str(&line); buf.push('\n');
        }
        buf
    });
    let mut out = String::new();
    let mut l = BufReader::new(so).lines();
    while let Ok(Some(line)) = l.next_line().await {
        app.emit("job-log", json!({ "id": id, "line": line })).ok();
        out.push_str(&line); out.push('\n');
    }
    let status = child.wait().await.map_err(err)?;
    let e = errs.await.unwrap_or_default();
    Ok((status.success(), if status.success() { out } else { format!("{out}\n{e}") }))
}

/// Prefix the command with `MODEL='…'` (shell-quoted) so templates can use ${MODEL:+--model "$MODEL"}.
fn with_model(c: &Connection, agent: &str, script: String) -> String {
    let m = setting(c, &format!("model_{agent}"));
    format!("MODEL='{}'\n{script}", m.trim().replace('\'', r"'\''"))
}

/// Inline context so the agent needs no tool calls: `{{skills}}` and `{{file:relative/path}}`.
fn expand_prompt(prompt: &str, dir: &Path) -> R<String> {
    let skills = list_skills()?.into_iter().map(|s| s.content.trim().to_string()).collect::<Vec<_>>().join("\n\n---\n\n");
    let re = regex::Regex::new(r"\{\{file:([^}]+)\}\}").unwrap();
    let out = re.replace_all(prompt, |c: &regex::Captures| {
        let rel = c[1].trim();
        if rel.contains("..") { return "(invalid path)".to_string(); }
        std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|_| "(file does not exist)".into())
    });
    let mut shots: Vec<String> = std::fs::read_dir(dir.join("input")).into_iter().flatten().flatten()
        .map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with("screenshot_"))
        .map(|n| format!("./input/{n}")).collect();
    shots.sort();
    let shots = if shots.is_empty() { "(none)".into() } else { shots.join(", ") };
    Ok(out.replace("{{skills}}", &skills).replace("{{screenshots}}", &shots))
}

/// LLMs often forget that `@` starts a reference in Typst: escape e-mail addresses (a@b.c → a\@b.c).
fn escape_typst_emails(src: &str) -> String {
    let re = regex::Regex::new(r"([A-Za-z0-9._%+\-])@([A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)+)").unwrap();
    re.replace_all(src, "$1\\@$2").into_owned()
}

/// The agent answers with `<<<FILE name>>> … <<<END>>>` blocks; write them inside `dir`.
/// Returns the written paths and the reply with file bodies collapsed (for the chat log).
/// Line based and forgiving: a missing `>>>`, a missing `<<<END>>>` (next block / end of output) and
/// markdown code fences around the content are all accepted.
fn write_file_blocks(out: &str, dir: &Path) -> R<(Vec<String>, String)> {
    let ansi = regex::Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap();
    let ok_name = regex::Regex::new(r"^[A-Za-z0-9_\-]+(/[A-Za-z0-9_\-]+)*\.(typ|json|md)$").unwrap();
    let out = ansi.replace_all(out, "");
    let (mut written, mut summary) = (vec![], vec![]);
    let mut cur: Option<(String, Vec<&str>)> = None;

    let mut flush = |cur: &mut Option<(String, Vec<&str>)>, summary: &mut Vec<String>| -> R<()> {
        let Some((name, mut lines)) = cur.take() else { return Ok(()) };
        // strip a surrounding ``` fence
        while lines.first().is_some_and(|l| l.trim().is_empty()) { lines.remove(0); }
        while lines.last().is_some_and(|l| l.trim().is_empty()) { lines.pop(); }
        if lines.first().is_some_and(|l| l.trim_start().starts_with("```")) && lines.last().is_some_and(|l| l.trim() == "```") && lines.len() >= 2 {
            lines.remove(0);
            lines.pop();
        }
        if !ok_name.is_match(&name) || lines.is_empty() { summary.push(format!("[ignored block {name}]")); return Ok(()); }
        let mut body = lines.join("\n");
        if name.ends_with(".typ") { body = escape_typst_emails(&body); }
        let path = dir.join(&name);
        if let Some(p) = path.parent() { std::fs::create_dir_all(p).map_err(err)?; }
        std::fs::write(&path, body + "\n").map_err(err)?;
        summary.push(format!("[wrote {name}]"));
        written.push(name);
        Ok(())
    };

    for line in out.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("<<<FILE") {
            flush(&mut cur, &mut summary)?;
            let name = rest.trim().trim_end_matches('>').trim().trim_start_matches("./").to_string();
            cur = Some((name, vec![]));
        } else if t.starts_with("<<<END") {
            flush(&mut cur, &mut summary)?;
        } else if let Some((_, lines)) = cur.as_mut() {
            lines.push(line);
        } else {
            summary.push(line.to_string());
        }
    }
    flush(&mut cur, &mut summary)?;
    Ok((written, summary.join("\n").trim().to_string()))
}

async fn agent_in(app: &tauri::AppHandle, script: String, dir: &Path, id: i64, prompt: &str) -> R<(bool, String)> {
    if script.trim().is_empty() { return Err("No agent command configured (Settings → AI agent)".into()); }
    write_skills(dir)?;
    std::fs::write(dir.join(".prompt.md"), expand_prompt(prompt, dir)?).map_err(err)?;
    let started = std::time::Instant::now();
    let (ok, out) = run_streaming(app, id, &script, dir).await?;
    if !ok { return Ok((false, out)); }
    let (written, summary) = write_file_blocks(&out, dir)?;
    app.emit("job-log", json!({ "id": id, "line": format!("— done in {:.0}s, wrote {} file(s)", started.elapsed().as_secs_f32(), written.len()) })).ok();
    Ok((true, summary))
}

/// Send a prompt to the configured AI CLI inside the job directory.
#[tauri::command]
async fn run_agent(app: tauri::AppHandle, db: State<'_, Db>, id: i64, prompt: String, label: String, cont: bool) -> R<String> {
    let (dir, script) = {
        let c = db.0.lock().unwrap();
        let agent = setting(&c, "agent");
        let script = with_model(&c, &agent, setting(&c, &format!("cmd_{agent}{}", if cont { "_cont" } else { "" })));
        push_chat(&c, id, "user", &label);
        (job_dir(&c, id)?, script)
    };
    let (ok, out) = agent_in(&app, script, &dir, id, &prompt).await?;
    push_chat(&db.0.lock().unwrap(), id, if ok { "agent" } else { "error" }, out.trim());
    if ok { Ok(out) } else { Err(out) }
}

/// Run the AI in the master-CV directory (used to convert an imported CV into master.typ).
#[tauri::command]
async fn run_cv_agent(app: tauri::AppHandle, db: State<'_, Db>, prompt: String) -> R<String> {
    let script = {
        let c = db.0.lock().unwrap();
        let agent = setting(&c, "agent");
        with_model(&c, &agent, setting(&c, &format!("cmd_{agent}")))
    };
    let (ok, out) = agent_in(&app, script, &cv_dir(), 0, &prompt).await?;
    if ok { Ok(out) } else { Err(out) }
}

/// Save a clipboard-pasted image (base64) to a temp file so it can be attached like a picked screenshot.
#[tauri::command]
fn save_paste(data: String, ext: String) -> R<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(err)?;
    let dir = root().join("tmp");
    std::fs::create_dir_all(&dir).map_err(err)?;
    let ext: String = ext.chars().filter(char::is_ascii_alphanumeric).take(5).collect();
    let p = dir.join(format!("paste-{}.{ext}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(err)?.as_millis()));
    std::fs::write(&p, bytes).map_err(err)?;
    Ok(p.display().to_string())
}

/// Write a user-chosen file (path comes from a save dialog).
#[tauri::command]
fn save_text(path: String, content: String) -> R<()> {
    std::fs::write(path, content).map_err(err)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Db(Mutex::new(init_db())))
        .invoke_handler(tauri::generate_handler![
            get_settings, set_setting, reset_setting,
            list_skills, save_skill, delete_skill, restore_default_skills,
            read_cv_file, write_cv_file, restore_default_cv_file, import_cv_source, preview_cv_file,
            list_companies, add_companies, import_companies_csv, delete_company, clear_companies,
            fetch_url, list_jobs, get_job, create_job, ingest_job_info, update_job, delete_job,
            list_outputs, prepare_job_cv, compile_job, get_posting, read_job_file, write_job_file, preview_live, preview_close, refresh_snapshot, get_snapshot, clear_chat, list_models, run_agent, run_cv_agent, save_paste, save_text
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_and_match() {
        let rows = parse_csv("\u{feff}Firma;Notiz\n\"Acme GmbH\";\"applied; 2025\"\nFoo AG;\n");
        assert_eq!(rows[1], vec!["Acme GmbH", "applied; 2025"]);
        assert_eq!(rows.len(), 3);
        assert_eq!(norm("ACME GmbH & Co. KG"), "acme");
        assert_eq!(company_slug("Müller & Söhne GmbH"), "Müller_Söhne_GmbH");
        assert_eq!(escape_typst_emails("jobs@nordwind.example · a\\@b.de · #import \"@preview/x:0.1\""),
            "jobs\\@nordwind.example · a\\@b.de · #import \"@preview/x:0.1\"");
        let dir = std::env::temp_dir().join("applytool-test-blocks");
        let (w, sum) = write_file_blocks("<<<FILE cv/A.typ>>>\n```typst\nmail: x@y.com\n```\n<<<END>>>\n<<<FILE ../evil.typ>>>\nx\n<<<END>>>\ndone", &dir).unwrap();
        assert_eq!(w, vec!["cv/A.typ"]);
        assert_eq!(std::fs::read_to_string(dir.join("cv/A.typ")).unwrap(), "mail: x\\@y.com\n");
        assert!(sum.contains("[wrote cv/A.typ]") && sum.ends_with("done"));
        // header without ">>>" and no <<<END>>> before the next block / end of output (seen with Claude)
        let (w, _) = write_file_blocks("Here:\n<<<FILE cv/B.typ\nb\n<<<FILE cv/C.typ>>>\nc\n", &dir).unwrap();
        assert_eq!(w, vec!["cv/B.typ", "cv/C.typ"]);
        assert_eq!(std::fs::read_to_string(dir.join("cv/B.typ")).unwrap(), "b\n");
    }
}

#[cfg(test)]
mod e2e {
    use super::*;
    /// Manual benchmark: E2E_DIR=/tmp/e2e E2E_PROMPT=/tmp/p.md E2E_CMD='opencode run -m … "$(cat .prompt.md)"' cargo test e2e -- --ignored --nocapture
    #[test]
    #[ignore]
    fn e2e_agent_roundtrip() {
        let dir = PathBuf::from(std::env::var("E2E_DIR").unwrap());
        let prompt = std::fs::read_to_string(std::env::var("E2E_PROMPT").unwrap()).unwrap();
        std::fs::write(dir.join(".prompt.md"), expand_prompt(&prompt, &dir).unwrap()).unwrap();
        let t = std::time::Instant::now();
        let out = std::process::Command::new("bash").args(["-lc", &std::env::var("E2E_CMD").unwrap()]).current_dir(&dir).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let (written, summary) = write_file_blocks(&text, &dir).unwrap();
        println!("TIME {:.1}s  WROTE {:?}\n{}", t.elapsed().as_secs_f32(), written, summary.chars().take(600).collect::<String>());
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    /// BENCH_FILE=…/cv/CV_en.typ cargo test --release bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_live_preview() {
        let f = PathBuf::from(std::env::var("BENCH_FILE").unwrap());
        let src = std::fs::read_to_string(&f).unwrap();
        let fonts = root().join("fonts");
        let t = |label: &str, content: &str| {
            let t = std::time::Instant::now();
            let v = preview::render_changed(&f, content, &fonts, 60.0);
            println!("{label:<28} {:>6.1} ms  pages {} changed {} err {}", t.elapsed().as_secs_f64() * 1000.0,
                v["count"], v["changed"].as_array().unwrap().len(), v["error"]);
        };
        t("first open (fonts + full)", &src);
        let mut cur = src.clone();
        for i in 0..4 {
            // simulate typing at the end of the document (last page changes)
            cur.push_str(&format!(" typed{i}"));
            t(&format!("keystroke {i} (end of doc)"), &cur);
        }
        let near_top = src.replacen("\n", "\nX ", 30);
        t("edit near the top", &near_top);
        t("broken markup", &format!("{cur}\n#entry("));
    }
}
