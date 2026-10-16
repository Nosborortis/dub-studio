//! The journal (VideoSOS `journal.rs`, Speech Studio): what agents changed and what the window told the person, kept across restarts in
//! `<root>/journal.jsonl`. An agent's change is kept as facts (what was done, to what kind of thing, its name) for the
//! window to put into words of its language; a message the window showed is kept in the words it was shown in.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use axum::extract::Path as UrlPath;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// How many of the latest lines are kept.
pub const KEPT: usize = 500;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    pub id: u64,
    /// Milliseconds since 1970.
    pub at: i64,
    /// `agent` for a change an agent or the assistant made, `studio` for a message the window showed.
    pub source: String,
    /// `work`, `done`, `error` or `agent`.
    pub tone: String,
    pub verb: String,
    pub kind: String,
    pub target: String,
    pub text: String,
}

struct Journal {
    root: PathBuf,
    path: PathBuf,
    lines: Mutex<Vec<Line>>,
    /// The next line's id: it only grows, so an id a window has seen is never given to another line.
    next: AtomicU64,
}

static JOURNAL: OnceLock<Journal> = OnceLock::new();

/// Opens the journal of the studio's root once at start; a line that does not read is reported and skipped.
pub fn open(root: &Path) {
    let path = root.join("journal.jsonl");
    let mut lines = Vec::new();
    if let Ok(text) = std::fs::read_to_string(&path) {
        for (n, row) in text.lines().enumerate().filter(|(_, row)| !row.trim().is_empty()) {
            match serde_json::from_str::<Line>(row) {
                Ok(line) => lines.push(line),
                Err(e) => tracing::error!("[ERROR] {}:{}: {e}", path.display(), n + 1),
            }
        }
    }
    let next = lines.iter().map(|l| l.id).max().unwrap_or(0) + 1;
    let _ = JOURNAL.set(Journal { root: root.to_path_buf(), path, lines: Mutex::new(lines), next: AtomicU64::new(next) });
}

fn journal() -> Result<&'static Journal, String> {
    JOURNAL.get().ok_or_else(|| "the journal is not open".to_string())
}

/// The name of a project the journal names instead of its id: its source file, as the projects list shows it.
pub fn project_name(pid: &str) -> Option<String> {
    if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let dir = journal().ok()?.root.join("workspace").join(pid);
    let text = std::fs::read_to_string(dir.join("project.json")).ok()?;
    let video = serde_json::from_str::<serde_json::Value>(&text).ok()?.pointer("/meta/video")?.as_str()?.to_string();
    Some(crate::source_name(&dir, &video)).filter(|n| !n.trim().is_empty())
}

fn write_all(j: &Journal, lines: &[Line]) -> Result<(), String> {
    let mut text = String::new();
    for line in lines {
        text.push_str(&serde_json::to_string(line).map_err(|e| e.to_string())?);
        text.push('\n');
    }
    let tmp = j.path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &j.path).map_err(|e| format!("{}: {e}", j.path.display()))
}

/// Writes a line, keeping the latest ones, and tells the windows.
pub fn note(line: Line) -> Result<Line, String> {
    let j = journal()?;
    let line = {
        let mut lines = j.lines.lock().unwrap_or_else(|p| p.into_inner());
        let line = Line { id: j.next.fetch_add(1, Ordering::Relaxed), at: chrono::Utc::now().timestamp_millis(), ..line };
        lines.push(line.clone());
        if lines.len() > KEPT {
            let extra = lines.len() - KEPT;
            lines.drain(..extra);
            write_all(j, &lines)?;
        } else {
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&j.path).map_err(|e| format!("{}: {e}", j.path.display()))?;
            writeln!(file, "{}", serde_json::to_string(&line).map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", j.path.display()))?;
        }
        line
    };
    crate::mcp::tell_windows(json!({ "changed": "journal", "by": crate::mcp::current_actor() }));
    Ok(line)
}

/// An agent's change, journaled; a failure to write is logged, the change itself stands.
pub fn agent_change(verb: &str, kind: &str, target: &str) {
    let line = Line { source: "agent".into(), tone: "agent".into(), verb: verb.into(), kind: kind.into(), target: target.into(), ..Default::default() };
    if let Err(e) = note(line) {
        tracing::error!("[ERROR] the journal did not take an agent's change ({verb} {kind}): {e}");
    }
}

fn failed(e: String) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
}

/// GET /journal: the lines, oldest first.
pub async fn read() -> Response {
    match journal() {
        Ok(j) => Json(j.lines.lock().unwrap_or_else(|p| p.into_inner()).clone()).into_response(),
        Err(e) => failed(e),
    }
}

/// POST /journal: a line of the window (`studio`, its text) or of an agent (verb and kind).
pub async fn write(Json(line): Json<Line>) -> Response {
    let complete = match line.source.as_str() {
        "agent" => !line.verb.is_empty() && !line.kind.is_empty(),
        "studio" => !line.text.trim().is_empty() && matches!(line.tone.as_str(), "work" | "done" | "error" | "agent"),
        _ => false,
    };
    if !complete {
        return (StatusCode::BAD_REQUEST, "a journal line is an agent's change (verb and kind) or a window's message (text and tone work, done, error or agent)").into_response();
    }
    match note(line) {
        Ok(line) => Json(line).into_response(),
        Err(e) => failed(e),
    }
}

/// DELETE /journal: every line.
pub async fn clear() -> Response {
    let result = journal().and_then(|j| {
        let mut lines = j.lines.lock().unwrap_or_else(|p| p.into_inner());
        lines.clear();
        write_all(j, &lines)
    });
    match result {
        Ok(()) => {
            crate::mcp::tell_windows(json!({ "changed": "journal" }));
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => failed(e),
    }
}

/// DELETE /journal/{id}: one line.
pub async fn remove(UrlPath(id): UrlPath<u64>) -> Response {
    let result = journal().and_then(|j| {
        let mut lines = j.lines.lock().unwrap_or_else(|p| p.into_inner());
        let before = lines.len();
        lines.retain(|l| l.id != id);
        if lines.len() == before {
            return Ok(false);
        }
        write_all(j, &lines).map(|()| true)
    });
    match result {
        Ok(true) => {
            crate::mcp::tell_windows(json!({ "changed": "journal" }));
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, "no such journal line").into_response(),
        Err(e) => failed(e),
    }
}
