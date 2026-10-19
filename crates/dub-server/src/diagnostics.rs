//! Diagnostics (Higgs-Ultimate, rusvoice doctor): the version, the system, the graphics card against CUDA 13, ffmpeg,
//! every component and whether it is on disk, what the engines' DLLs are missing, and the server's latest log lines -
//! as text to copy, and as a bug-report pack (`<root>/reports/*.zip`: the diagnostics, the activity journal and the log).
//! Keys never get in: they are not in the report, and anything in the log that looks like one is masked.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

use crate::AppState;

/// How many of the server's latest log lines are kept for the report.
const LOG_LINES: usize = 400;

static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// A tracing layer keeping the latest log lines (info and above) for the report.
pub struct RecentLog;

struct Message(String);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}{}", self.0);
        } else {
            self.0.push_str(&format!(" {}={value:?}", field.name()));
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0 = format!("{value}{}", self.0);
        } else {
            self.0.push_str(&format!(" {}={value}", field.name()));
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for RecentLog {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if *meta.level() > tracing::Level::INFO {
            return;
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        let line = format!("{} {:>5} {}: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), meta.level(), meta.target(), message.0);
        let mut lines = LOG.lock().unwrap_or_else(|p| p.into_inner());
        lines.push_back(masked(&line));
        while lines.len() > LOG_LINES {
            lines.pop_front();
        }
    }
}

/// Anything that looks like a key masked: OpenRouter, the studio's own API, Google, fal, Hugging Face, bearer headers.
fn masked(text: &str) -> String {
    static KEYS: OnceLock<regex::Regex> = OnceLock::new();
    let keys = KEYS.get_or_init(|| regex::Regex::new(r"(sk-[A-Za-z0-9_-]{8,}|AIza[0-9A-Za-z_-]{20,}|hf_[A-Za-z0-9]{20,}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}:[0-9a-f]{20,}|(?i:bearer)\s+[A-Za-z0-9._-]{12,})").expect("key pattern"));
    keys.replace_all(text, "[key]").into_owned()
}

fn recent_log() -> Vec<String> {
    LOG.lock().unwrap_or_else(|p| p.into_inner()).iter().cloned().collect()
}

/// The process's logging: the console (info and above, RUST_LOG overrides) and the latest lines kept for the report.
/// The service binary and the desktop app call it once at start.
pub fn init_logging() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let started = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,ort=warn".into()))
        .with(tracing_subscriber::fmt::layer())
        .with(RecentLog)
        .try_init();
    if let Err(e) = started {
        eprintln!("[ERROR] logging was set up before: {e}");
    }
}

/// The diagnostics as data.
fn report(root: &Path) -> Value {
    let gpu = crate::hw::gpu_report();
    let hw = crate::hw::snapshot();
    let setup = crate::setup::setup_status(root);
    let components: Vec<Value> = setup.components.iter().map(|c| json!({ "id": c.id, "installed": c.installed, "size": c.size, "on_disk": c.bytes_on_disk })).collect();
    #[cfg(windows)]
    // ffmpeg found on PATH counts as installed (setup's rule), so its own files are not checked
    let installed: Vec<&str> = setup.components.iter().filter(|c| c.installed && !(c.id == "ffmpeg" && crate::setup::ffmpeg_on_path() && c.bytes_on_disk == 0)).map(|c| c.id.as_str()).collect();
    #[cfg(windows)]
    let dll = match crate::dll_imports::check_components(root, &|id| installed.contains(&id), &mut |_, _| {}) {
        Ok(missing) => json!(missing),
        Err(e) => json!([format!("the check failed: {e}")]),
    };
    #[cfg(not(windows))]
    let dll = json!([]);
    json!({
        "version": crate::service::app_version(),
        "os": format!("{} ({})", sysinfo::System::long_os_version().or_else(sysinfo::System::name).unwrap_or_else(|| std::env::consts::OS.to_string()), std::env::consts::ARCH),
        "gpu": gpu,
        "memory": { "vram_total": hw.total_vram, "vram_free": hw.free_vram, "ram_total": hw.total_ram, "ram_used": hw.used_ram },
        "ffmpeg_on_path": crate::setup::ffmpeg_on_path(),
        "models_dir": setup.models_dir,
        "free_bytes": setup.free_bytes,
        "ready": setup.ready,
        "components": components,
        "dll_missing": dll,
        "data_dir": root.to_string_lossy(),
    })
}

/// The diagnostics as text to paste into an issue.
fn text(report: &Value, log: &[String]) -> String {
    let gb = |b: &Value| b.as_u64().map(|b| format!("{:.1} GB", b as f64 / crate::hw::GIB)).unwrap_or_else(|| "?".into());
    let gpu = &report["gpu"];
    let mut out = String::new();
    out.push_str(&format!("Dub Studio {}\n", report["version"].as_str().unwrap_or_default()));
    out.push_str(&format!("OS: {}\n", report["os"].as_str().unwrap_or_default()));
    out.push_str(&format!(
        "GPU: {} · driver {} · CUDA {} · CC {} · CUDA 13 build: {}\n",
        gpu["name"].as_str().unwrap_or("none"),
        gpu["driverVersion"].as_str().unwrap_or("?"),
        gpu["cudaDriver"].as_str().unwrap_or("?"),
        gpu["compute"].as_str().unwrap_or("?"),
        if gpu["cuda13Ok"] == true { "yes".to_string() } else { format!("no ({})", gpu["reason"].as_str().unwrap_or("?")) },
    ));
    out.push_str(&format!("VRAM: {} free of {} · RAM: {} used of {}\n", gb(&report["memory"]["vram_free"]), gb(&report["memory"]["vram_total"]), gb(&report["memory"]["ram_used"]), gb(&report["memory"]["ram_total"])));
    out.push_str(&format!("ffmpeg on PATH: {}\n", report["ffmpeg_on_path"]));
    out.push_str(&format!("Models: {} · free {} · ready: {}\n", report["models_dir"].as_str().unwrap_or_default(), gb(&report["free_bytes"]), report["ready"]));
    out.push_str("Components:\n");
    for c in report["components"].as_array().into_iter().flatten() {
        out.push_str(&format!("  {} {}\n", if c["installed"] == true { "[OK]" } else { "[--]" }, c["id"].as_str().unwrap_or_default()));
    }
    let missing = report["dll_missing"].as_array().cloned().unwrap_or_default();
    out.push_str(&format!("DLL check: {}\n", if missing.is_empty() { "nothing missing".to_string() } else { missing.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ") }));
    out.push_str(&format!("\nLatest log ({} lines):\n", log.len()));
    for line in log {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// GET /diagnostics: the report and its text.
pub async fn get(State(st): State<AppState>) -> Response {
    let root = st.repo_root.clone();
    match tokio::task::spawn_blocking(move || report(&root)).await {
        Ok(report) => {
            let log = recent_log();
            Json(json!({ "text": text(&report, &log), "report": report })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn pack(root: &Path) -> Result<PathBuf, String> {
    let report = report(root);
    let log = recent_log();
    let dir = root.join("reports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("dub-studio-report-{}.zip", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, body: &[u8]| -> Result<(), String> {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(body).map_err(|e| e.to_string())
    };
    put("diagnostics.txt", text(&report, &log).as_bytes())?;
    put("diagnostics.json", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?.as_bytes())?;
    if let Ok(journal) = std::fs::read_to_string(root.join("journal.jsonl")) {
        put("journal.jsonl", masked(&journal).as_bytes())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

/// POST /diagnostics/report: packs the bug report into the data folder and answers its path.
pub async fn make_report(State(st): State<AppState>) -> Response {
    let root = st.repo_root.clone();
    match tokio::task::spawn_blocking(move || pack(&root)).await {
        Ok(Ok(path)) => Json(json!({ "path": path.to_string_lossy() })).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "report_failed", "detail": e }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /diagnostics/report/reveal {path}: shows a report of the data folder in the file manager.
pub async fn reveal_report(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let dir = st.repo_root.join("reports");
    let asked = PathBuf::from(body.get("path").and_then(Value::as_str).unwrap_or_default());
    let inside = asked.parent().is_some_and(|p| p == dir) && asked.extension().is_some_and(|e| e == "zip") && asked.is_file();
    if !inside {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no_report", "detail": asked.to_string_lossy() }))).into_response();
    }
    crate::reveal_in_explorer(asked.to_string_lossy().into_owned());
    Json(json!({ "ok": true })).into_response()
}

/// POST /diagnostics/logs/reveal: the folder with the helpers' logs (llama-server and the like) in the file manager.
pub async fn reveal_logs(State(st): State<AppState>) -> Response {
    let dir = st.repo_root.join("logs");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "io", "detail": format!("{}: {e}", dir.display()) }))).into_response();
    }
    crate::reveal_in_explorer(dir.to_string_lossy().into_owned());
    Json(json!({ "ok": true })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_masked_in_the_log() {
        assert_eq!(masked("key sk-or-v1-abcdef1234567890 used"), "key [key] used");
        assert_eq!(masked("Authorization: Bearer abcdefghijklmnop"), "Authorization: [key]");
        assert_eq!(masked("AIzaSyA1234567890abcdefghijk ok"), "[key] ok");
        assert_eq!(masked("block 12 voiced in 3.2 s"), "block 12 voiced in 3.2 s");
    }

    #[test]
    fn the_text_names_the_card_and_every_component() {
        let report = json!({ "version": "1.0.0", "os": "Windows 11", "gpu": { "name": "RTX 4090", "driverVersion": "581.29", "cudaDriver": "13.0", "compute": "8.9", "cuda13Ok": true },
            "memory": { "vram_total": 24_000_000_000u64, "vram_free": 20_000_000_000u64, "ram_total": 64_000_000_000u64, "ram_used": 10_000_000_000u64 },
            "ffmpeg_on_path": true, "models_dir": "D:/m", "free_bytes": 100_000_000_000u64, "ready": true,
            "components": [{ "id": "higgs", "installed": true }, { "id": "drama3", "installed": false }], "dll_missing": [] });
        let t = text(&report, &["a log line".into()]);
        assert!(t.contains("RTX 4090") && t.contains("[OK] higgs") && t.contains("[--] drama3") && t.contains("nothing missing") && t.contains("a log line"));
    }
}
