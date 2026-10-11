//! Модели кастинга на бэкенде из DUB_FACES_BACKEND (cpu или gpu): время загрузки и прогона и эмбеддинги в
//! JSON — чтобы сравнить прогоны на процессоре и видеокарте.
//! Запуск: DUB_FACES_BACKEND=gpu cargo run -p dub-faces --example backends -- <models> <voice.wav> <frame.png>...

use std::time::Instant;

use dub_faces::{FacesModels, LvFace, Scrfd, VoiceEmbedder};

const RUNS: usize = 5;

fn median_ms(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, wav, frames @ ..] = args.as_slice() else {
        eprintln!("usage: backends <models> <voice.wav> <frame.png>...");
        std::process::exit(2);
    };
    let root = std::path::Path::new(root);
    let backend = std::env::var("DUB_FACES_BACKEND").unwrap_or_else(|_| "cpu".into());
    let models = FacesModels::resolve(root);

    let started = Instant::now();
    let mut voice = VoiceEmbedder::load(&dub_faces::wespeaker_path(root)).expect("WeSpeaker load");
    let mut scrfd = Scrfd::load(&models.scrfd).expect("SCRFD load");
    let mut lvface = LvFace::load(&models.lvface).expect("LVFace load");
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;

    let wav = std::path::Path::new(wav);
    let voice_embedding = voice.embed_wav(wav).expect("voice embed");
    let voice_ms = median_ms((0..RUNS).map(|_| {
        let t = Instant::now();
        voice.embed_wav(wav).expect("voice embed");
        t.elapsed().as_secs_f64() * 1000.0
    }).collect());

    let mut faces = Vec::new();
    for frame in frames {
        let image = image::open(frame).expect("frame").to_rgb8();
        let found = scrfd.detect(&image).expect("detect");
        let detect_ms = median_ms((0..RUNS).map(|_| {
            let t = Instant::now();
            scrfd.detect(&image).expect("detect");
            t.elapsed().as_secs_f64() * 1000.0
        }).collect());
        let Some(face) = found.iter().max_by(|a, b| a.score.total_cmp(&b.score)) else {
            faces.push(serde_json::json!({ "frame": frame, "faces": 0, "detect_ms": detect_ms }));
            continue;
        };
        let embedding = lvface.embed_face(&image, face).expect("face embed");
        let embed_ms = median_ms((0..RUNS).map(|_| {
            let t = Instant::now();
            lvface.embed_face(&image, face).expect("face embed");
            t.elapsed().as_secs_f64() * 1000.0
        }).collect());
        faces.push(serde_json::json!({
            "frame": frame, "faces": found.len(), "detect_ms": detect_ms, "embed_ms": embed_ms,
            "box": [face.x1, face.y1, face.x2, face.y2], "embedding": embedding,
        }));
    }

    println!("{}", serde_json::json!({
        "backend": backend, "load_ms": load_ms, "voice_ms": voice_ms, "voice": voice_embedding, "faces": faces,
    }));
}
