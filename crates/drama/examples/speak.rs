//! drama_engine.dll через FFI: синтез, отмена посреди фразы, синтез после отмены.
//! speak <dll> <cuda-libs-dir> <model-dir> <ref.wav> <ref-text> <text> <out-prefix>

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn write_wav(path: &str, audio: &[f32], sr: u32) {
    let data = (audio.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    for part in [&b"RIFF"[..], &(36 + data).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes(), &1u16.to_le_bytes(), &1u16.to_le_bytes(), &sr.to_le_bytes(), &(sr * 2).to_le_bytes(), &2u16.to_le_bytes(), &16u16.to_le_bytes(), b"data", &data.to_le_bytes()] {
        out.extend_from_slice(part);
    }
    for s in audio {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dll, libs, model, ref_wav, ref_text, text, out) = (&a[1], &a[2], &a[3], &a[4], &a[5], &a[6], &a[7]);
    let t = Instant::now();
    let engine = Arc::new(drama::DramaEngine::load(Path::new(dll), &[Path::new(libs)], Path::new(model), 2048).expect("load"));
    let sr = engine.sample_rate();
    println!("[OK] loaded {} in {:.1}s, {sr} Hz", engine.version(), t.elapsed().as_secs_f64());
    let params = drama::Params { seed: 42, ..Default::default() };

    let t = Instant::now();
    let audio = engine.speak(text, Path::new(ref_wav), ref_text, &params).expect("speak");
    let secs = audio.len() as f64 / sr as f64;
    println!("[OK] spoke {secs:.2}s in {:.2}s ({:.1}x)", t.elapsed().as_secs_f64(), secs / t.elapsed().as_secs_f64());
    write_wav(&format!("{out}-1.wav"), &audio, sr);

    let e = engine.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        e.cancel();
    });
    let t = Instant::now();
    match engine.speak(text, Path::new(ref_wav), ref_text, &params) {
        Err(drama::DramaError::Cancelled) => println!("[OK] cancelled after {:.2}s", t.elapsed().as_secs_f64()),
        other => println!("[ERROR] expected Cancelled, got {:?}", other.map(|a| a.len())),
    }
    canceller.join().unwrap();

    let t = Instant::now();
    let again = engine.speak(text, Path::new(ref_wav), ref_text, &params).expect("speak after cancel");
    println!("[OK] after cancel {:.2}s in {:.2}s, same as first: {}", again.len() as f64 / sr as f64, t.elapsed().as_secs_f64(), again == audio);
    write_wav(&format!("{out}-2.wav"), &again, sr);
}
