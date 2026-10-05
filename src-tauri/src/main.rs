#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--audit-editor-preview") {
        let file = std::path::Path::new(args.get(2).expect("document path required"));
        let source = std::fs::read_to_string(file).expect("read document");
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        for (label, content) in [("open", source.clone()), ("edit", format!("{source}\nPreview audit.")), ("invalid", format!("{source}\n#entry("))] {
            let started = std::time::Instant::now();
            let result = runtime.block_on(apply_tool_lib::render_editor_preview(file, &content)).expect("render preview");
            println!("{label}: {:.0}ms, pages={}, changed={}, error={}", started.elapsed().as_secs_f64() * 1000.0, result["count"], result["changed"].as_array().unwrap().len(), result["error"]);
            assert_eq!(result["error"].is_null(), label != "invalid");
        }
        return;
    }
    apply_tool_lib::run()
}
