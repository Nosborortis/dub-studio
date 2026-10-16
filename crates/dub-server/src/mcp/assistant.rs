//! The connected agent as the studio's language model (YuE2 `AssistantProvider::Agent`, Speech Studio
//! `mcp/assistant.rs`): with «Agent (MCP)» as the LLM provider, what the studio would ask its own model -
//! translating the lines, shortening them, the glossary - waits here as a request; the agent takes it with
//! `assistant_requests_wait` and answers with `assistant_request_answer`, and the studio goes on with the answer as
//! with its model's.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use super::{composite, object, text, Tool};

/// How long the studio's model work waits for the agent: under the 600 s its chat client waits.
const AGENT_AS_MODEL: Duration = Duration::from_secs(570);

struct AgentQuestion {
    question: Value,
    answer: tokio::sync::oneshot::Sender<String>,
}

static QUESTIONS: Mutex<Vec<AgentQuestion>> = Mutex::new(Vec::new());
static QUESTION_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "assistant_requests_wait",
            description: "When the person has made you Dub Studio's language model (Settings, Models: LLM provider 'Agent (MCP)'), the studio's own model work waits here as kind 'studio_model': translating the lines, shortening them, the glossary (target 'llm'), and looking at the video's frames for the subtitle style, titles and scene (target 'vision', also chosen as 'Agent (MCP)' there). Returns the waiting requests - id, kind, target, the instructions, the request, images (each frame as a file path on this computer and a URL of this server, named in the request as [image N: path]; read them as pictures) and answer_schema, the JSON schema the answer must match - as soon as there is one, or after seconds (30 by default, at most 55). Answer each yourself, as the studio's model would, by its instructions: with the JSON object its answer_schema describes, or plain text without one, through assistant_request_answer; keep calling while the studio works.",
            schema: || object(json!({ "seconds": { "type": "integer", "minimum": 1, "maximum": 55 } }), &[]),
            call: |_| composite("questions"),
        },
        Tool {
            name: "assistant_request_answer",
            description: "Answer a request from assistant_requests_wait: the JSON object its answer_schema describes (or plain text without one); the studio goes on with it as with its own model's answer.",
            schema: || object(json!({ "request_id": { "type": "string" }, "answer": { "anyOf": [{ "type": "object" }, { "type": "string" }] } }), &["request_id", "answer"]),
            call: |_| composite("answer"),
        },
    ]
}

/// How many requests wait for the agent, for the settings page.
pub(super) fn waiting() -> usize {
    open_questions().len()
}

fn open_questions() -> Vec<Value> {
    let mut waiting = QUESTIONS.lock().unwrap_or_else(|p| p.into_inner());
    waiting.retain(|question| !question.answer.is_closed());
    waiting.iter().map(|question| question.question.clone()).collect()
}

async fn ask_agent_within(question: Value, wait: Duration) -> Result<String, String> {
    if !super::agent_present() {
        return Err(t!("llm-agent-absent"));
    }
    let mut question = question;
    let id = match question["id"].as_str() {
        Some(id) => id.to_string(),
        None => next_id(),
    };
    question["id"] = json!(id);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    QUESTIONS.lock().unwrap_or_else(|p| p.into_inner()).push(AgentQuestion { question, answer: sender });
    match tokio::time::timeout(wait, receiver).await {
        Ok(Ok(answer)) => Ok(answer),
        _ => {
            QUESTIONS.lock().unwrap_or_else(|p| p.into_inner()).retain(|question| question.question["id"] != id.as_str());
            Err(t!("llm-agent-no-answer", minutes = (wait.as_secs() / 60) as i64))
        }
    }
}

pub(crate) async fn agent_llm(axum::Json(body): axum::Json<Value>) -> axum::response::Response {
    use axum::response::IntoResponse;
    let fail = |code: axum::http::StatusCode, message: String| (code, axum::Json(json!({ "error": { "message": message } }))).into_response();
    let id = next_id();
    // the frames of a vision request reach the agent as files it reads, kept until it answers
    let mut images: Vec<std::path::PathBuf> = Vec::new();
    let mut shown: Vec<Value> = Vec::new();
    let mut text_of = |m: &Value| -> Result<String, String> {
        match &m["content"] {
            Value::String(s) => Ok(s.clone()),
            Value::Array(parts) => {
                let mut out: Vec<String> = Vec::new();
                for part in parts {
                    if let Some(text) = part["text"].as_str() {
                        out.push(text.to_string());
                    } else if let Some(url) = part.pointer("/image_url/url").and_then(Value::as_str) {
                        let path = save_image(&id, images.len() + 1, url)?;
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                        out.push(format!("[image {}: {}]", images.len() + 1, path.display()));
                        shown.push(json!({ "path": path.display().to_string(), "url": crate::service::bound_port().map(|port| format!("http://127.0.0.1:{port}/mcp/agent-llm/images/{name}")) }));
                        images.push(path);
                    }
                }
                Ok(out.join("\n"))
            }
            _ => Ok(String::new()),
        }
    };
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let mut instructions: Vec<String> = Vec::new();
    let mut request: Vec<String> = Vec::new();
    for message in &messages {
        match text_of(message) {
            Ok(text) if message["role"] == "system" => instructions.push(text),
            Ok(text) => request.push(text),
            Err(error) => {
                images.iter().for_each(|path| { let _ = std::fs::remove_file(path); });
                return fail(axum::http::StatusCode::BAD_REQUEST, error);
            }
        }
    }
    let _frames = Frames(images);
    let request = request.join("\n\n");
    let schema = body.pointer("/response_format/json_schema/schema").cloned();
    let target = body["model"].as_str().and_then(|model| model.strip_prefix("agent-")).unwrap_or("llm");
    let question = json!({ "id": id, "kind": "studio_model", "target": target, "instructions": instructions.join("\n\n"), "request": request, "images": shown, "answer_schema": schema });
    let answer = if request.is_empty() {
        Err((axum::http::StatusCode::BAD_REQUEST, "messages carry no request".to_string()))
    } else if !super::agent_present() {
        Err((axum::http::StatusCode::CONFLICT, t!("llm-agent-absent")))
    } else {
        ask_agent_within(question, AGENT_AS_MODEL).await.map_err(|e| (axum::http::StatusCode::GATEWAY_TIMEOUT, e))
    };
    match answer {
        Ok(answer) => axum::Json(json!({ "choices": [{ "message": { "role": "assistant", "content": answer }, "finish_reason": "stop" }] })).into_response(),
        Err((code, message)) => fail(code, message),
    }
}

/// The frames of one request: gone with it, answered, failed or dropped when the studio stops waiting.
struct Frames(Vec<std::path::PathBuf>);

impl Drop for Frames {
    fn drop(&mut self) {
        self.0.iter().for_each(|path| { let _ = std::fs::remove_file(path); });
    }
}

fn next_id() -> String {
    format!("q{}", QUESTION_ID.fetch_add(1, Ordering::Relaxed))
}

/// Where the frames of the agent's requests wait while it answers.
fn images_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("dub-studio-agent-llm")
}

/// A `data:image/...;base64,` picture of a request as a file of its own.
fn save_image(id: &str, n: usize, url: &str) -> Result<std::path::PathBuf, String> {
    use base64::Engine;
    let (head, data) = url.split_once(',').ok_or("an image of the request is not a data URL")?;
    let ext = match head.trim_start_matches("data:").split(';').next().unwrap_or_default() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    };
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).map_err(|e| format!("an image of the request is not base64: {e}"))?;
    let dir = images_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{id}-{n}.{ext}"));
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// A frame of a waiting request, for an agent that reads pictures by address.
pub(crate) async fn agent_llm_image(axum::extract::Path(name): axum::extract::Path<String>) -> axum::response::Response {
    use axum::response::IntoResponse;
    let safe = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') && !name.contains("..");
    let mime = match name.rsplit('.').next() {
        Some("jpg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    match (safe, std::fs::read(images_dir().join(&name))) {
        (true, Ok(bytes)) => ([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response(),
        _ => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn wait_for_questions(args: &Value) -> Value {
    let seconds = args.get("seconds").and_then(Value::as_u64).unwrap_or(30).clamp(1, 55);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        let waiting = open_questions();
        if !waiting.is_empty() {
            return json!({ "requests": waiting });
        }
        if tokio::time::Instant::now() >= deadline {
            return json!({ "requests": [], "note": "no request yet; call assistant_requests_wait again to keep listening" });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

pub(super) fn answer_question(args: &Value) -> Result<Value, String> {
    let id = text(args, "request_id")?;
    let answer = match args.get("answer") {
        Some(Value::String(text)) => text.clone(),
        Some(value) if !value.is_null() => value.to_string(),
        _ => return Err("'answer' is required".into()),
    };
    let found = {
        let mut waiting = QUESTIONS.lock().unwrap_or_else(|p| p.into_inner());
        waiting.iter().position(|question| question.question["id"] == id.as_str()).map(|at| waiting.remove(at))
    };
    let Some(question) = found else {
        return Err(format!("No request {id} is waiting; assistant_requests_wait lists the open ones."));
    };
    question.answer.send(answer).map_err(|_| format!("Request {id} was given up by the studio before the answer came."))?;
    Ok(json!({ "answered": id, "note": "The studio goes on with it as with its own model's answer." }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_answer_reaches_the_question_it_names() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        QUESTIONS.lock().unwrap().push(AgentQuestion { question: json!({ "id": "q-test", "request": "hi" }), answer: sender });
        assert!(open_questions().iter().any(|q| q["id"] == "q-test"));
        assert!(answer_question(&json!({ "request_id": "q-nobody", "answer": "x" })).is_err());
        answer_question(&json!({ "request_id": "q-test", "answer": "привет" })).unwrap();
        assert_eq!(receiver.await.unwrap(), "привет");
        assert!(!open_questions().iter().any(|q| q["id"] == "q-test"));
    }

    #[tokio::test]
    async fn the_studios_model_work_reaches_the_agent_with_its_schema_and_comes_back_as_the_models_answer() {
        super::super::seen("test");
        let schema = json!({ "type": "object", "properties": { "lines": { "type": "array" } } });
        let body = json!({ "model": "agent", "messages": [{ "role": "system", "content": "Translate the lines." }, { "role": "user", "content": [{ "type": "text", "text": "Hello there." }] }], "response_format": { "type": "json_schema", "json_schema": { "name": "answer", "schema": schema } } });
        let call = tokio::spawn(agent_llm(axum::Json(body)));
        let question = loop {
            if let Some(q) = open_questions().into_iter().find(|q| q["kind"] == "studio_model" && q["request"] == "Hello there.") {
                break q;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(question["instructions"], "Translate the lines.");
        assert_eq!(question["answer_schema"], schema);
        answer_question(&json!({ "request_id": question["id"], "answer": { "lines": [] } })).unwrap();
        let response = call.await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let answer: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(answer["choices"][0]["message"]["content"], r#"{"lines":[]}"#);
        assert_eq!(answer["choices"][0]["finish_reason"], "stop");
    }

    #[tokio::test]
    async fn the_frames_of_a_vision_request_reach_the_agent_as_files_and_go_when_it_answers() {
        use base64::Engine;
        super::super::seen("test");
        let png = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nframe");
        let body = json!({ "model": "agent-vision", "messages": [{ "role": "user", "content": [{ "type": "text", "text": "What is on the frame?" }, { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{png}") } }] }] });
        let call = tokio::spawn(agent_llm(axum::Json(body)));
        let question = loop {
            if let Some(q) = open_questions().into_iter().find(|q| q["target"] == "vision") {
                break q;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let path = std::path::PathBuf::from(question["images"][0]["path"].as_str().expect("a frame's path"));
        assert_eq!(std::fs::read(&path).unwrap(), b"\x89PNG\r\n\x1a\nframe");
        assert!(question["request"].as_str().unwrap().contains(&format!("[image 1: {}]", path.display())));
        answer_question(&json!({ "request_id": question["id"], "answer": "a street at night" })).unwrap();
        assert_eq!(call.await.unwrap().status(), axum::http::StatusCode::OK);
        assert!(!path.exists(), "the frame is removed once answered");
    }
}
