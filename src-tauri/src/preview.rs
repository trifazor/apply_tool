//! Fast live preview for the document editor.
//!
//! Instead of spawning the typst CLI for every keystroke, the compiler runs in-process:
//! fonts are loaded once, the edited source is updated in place (typst's memoization then
//! only re-lays-out what changed), and only pages whose content changed are rasterised
//! and sent to the UI.

use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

/// Fonts are discovered once per app run (embedded + system + ~/ApplyTool/fonts).
fn fonts(extra: &Path) -> &'static FontStore {
    static FONTS: OnceLock<FontStore> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut store = FontStore::new();
        store.extend(typst_kit::fonts::embedded());
        store.extend(typst_kit::fonts::system());
        store.extend(typst_kit::fonts::scan(extra));
        store
    })
}

fn library() -> &'static LazyHash<Library> {
    static LIB: OnceLock<LazyHash<Library>> = OnceLock::new();
    LIB.get_or_init(|| LazyHash::new(Library::builder().build()))
}

struct EditorWorld {
    root: PathBuf,
    main: FileId,
    source: Source,
    fonts: &'static FontStore,
    files: Mutex<HashMap<FileId, Bytes>>,
}

impl EditorWorld {
    fn path(&self, id: FileId) -> FileResult<PathBuf> {
        let rooted = id.get();
        if !matches!(rooted.root(), VirtualRoot::Project) {
            return Err(FileError::Other(Some("packages are not available in the preview".into())));
        }
        rooted.vpath().realize(&self.root).map_err(|_| FileError::AccessDenied)
    }
}

impl World for EditorWorld {
    fn library(&self) -> &LazyHash<Library> { library() }
    fn book(&self) -> &LazyHash<FontBook> { self.fonts.book() }
    fn main(&self) -> FileId { self.main }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main { return Ok(self.source.clone()); }
        let path = self.path(id)?;
        let text = std::fs::read_to_string(&path).map_err(|e| FileError::from_io(e, &path))?;
        Ok(Source::new(id, text))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main { return Ok(Bytes::from_string(self.source.clone())); }
        if let Some(b) = self.files.lock().unwrap().get(&id) { return Ok(b.clone()); }
        let path = self.path(id)?;
        let b = Bytes::new(std::fs::read(&path).map_err(|e| FileError::from_io(e, &path))?);
        self.files.lock().unwrap().insert(id, b.clone());
        Ok(b)
    }

    fn font(&self, index: usize) -> Option<Font> { self.fonts.font(index) }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64
            + offset.map_or(0, |o| o.seconds() as i64);
        let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
        Datetime::from_ymd(y, m, d)
    }
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    ((yoe + era * 400 + i64::from(m <= 2)) as i32, m, d)
}

/// One editor session: the world plus the hash of every page rendered last time.
struct Session {
    file: PathBuf,
    world: EditorWorld,
    hashes: Vec<u128>,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

/// Compile `content` as if it were `file` and return only the pages that changed since the previous call
/// for the same file: `{ "count": n, "changed": [[index, base64png], …], "error": "…" | null }`.
pub fn render_changed(file: &Path, content: &str, fonts_dir: &Path, ppi: f32) -> Value {
    let mut guard = SESSION.lock().unwrap();
    let fresh = guard.as_ref().is_none_or(|s| s.file != file);
    if fresh {
        let root = file.parent().and_then(Path::parent).unwrap_or(Path::new("/")).to_path_buf();
        let rel = file.strip_prefix(&root).unwrap_or(file).to_string_lossy().replace('\\', "/");
        let main = match VirtualPath::new(format!("/{rel}")) {
            Ok(v) => RootedPath::new(VirtualRoot::Project, v).intern(),
            Err(e) => return json!({ "count": 0, "changed": [], "error": format!("{e:?}") }),
        };
        let world = EditorWorld { root, main, source: Source::new(main, content.to_string()), fonts: fonts(fonts_dir), files: Mutex::default() };
        *guard = Some(Session { file: file.to_path_buf(), world, hashes: vec![] });
    }
    let s = guard.as_mut().unwrap();
    if !fresh { s.world.source.replace(content); }

    let result = typst::compile::<PagedDocument>(&s.world).output;
    comemo::evict(30);
    let doc = match result {
        Ok(doc) => doc,
        Err(diags) => {
            let msg = diags.iter().map(|d| d.message.to_string()).collect::<Vec<_>>().join("\n");
            return json!({ "count": s.hashes.len(), "changed": [], "error": msg });
        }
    };
    let opts = typst_render::RenderOptions { pixel_per_pt: typst::utils::Scalar::new((ppi / 72.0) as f64), ..Default::default() };
    let mut changed = vec![];
    let mut hashes = Vec::with_capacity(doc.pages().len());
    for (i, page) in doc.pages().iter().enumerate() {
        let h = typst::utils::hash128(page);
        if s.hashes.get(i) != Some(&h) {
            if let Ok(png) = typst_render::render(page, &opts).encode_png() {
                changed.push(json!([i, base64::engine::general_purpose::STANDARD.encode(png)]));
            }
        }
        hashes.push(h);
    }
    s.hashes = hashes;
    json!({ "count": doc.pages().len(), "changed": changed, "error": null })
}

/// Forget the session (editor closed) so the next open starts clean and memory is released.
pub fn reset() {
    *SESSION.lock().unwrap() = None;
    comemo::evict(0);
}

/// Load fonts in the background at startup so the first editor preview is instant.
pub fn warm_up(fonts_dir: PathBuf) {
    std::thread::spawn(move || { fonts(&fonts_dir).book(); });
}
