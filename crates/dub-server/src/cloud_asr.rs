//! Облачная транскрипция (ASR) через OpenRouter (`/audio/transcriptions`, verbose_json с сегментами) напрямую
//! из Rust. Возвращает сегменты (start, end, text) — analyze раздаёт их по спикерам через диаризацию, как
//! локальный Parakeet/Whisper. Тяжёлые локальные ASR-модели тогда не нужны (облачный пресет для слабых ПК).

use std::path::Path;

use dub_llm::openrouter::OpenRouter;

/// Транскрибировать wav (16-бит PCM) выбранной STT-моделью OpenRouter -> сегменты (start, end, text). Нет ключа/модели ->
/// Err. `src_lang` — ISO-639-1 ("ru"/"en"/…) или "auto"/"" (авто-детект). Кусок без сегментов (не verbose_json) —
/// один сегмент на весь кусок.
pub fn transcribe(models_root: &Path, wav: &Path, src_lang: &str) -> Result<Vec<(f64, f64, String)>, String> {
    let key = crate::models::openrouter_key().ok_or_else(|| t!("cloud-asr-no-key"))?;
    let model = crate::models::openrouter_model(models_root, "asr");
    if model.is_empty() {
        return Err(t!("cloud-asr-no-model"));
    }
    let failed = |e: String| t!("cloud-asr-failed", error = e);
    let mut reader = hound::WavReader::open(wav).map_err(|e| failed(format!("{}: {e}", wav.display())))?;
    let spec = reader.spec();
    let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>().map_err(|e| failed(format!("{}: {e}", wav.display())))?;
    let rate = spec.sample_rate.max(1) as usize * spec.channels.max(1) as usize;
    let client = OpenRouter::new(Some(key)).map_err(|e| failed(format!("{e:#}")))?;

    let mut out: Vec<(f64, f64, String)> = Vec::new();
    for (start, end) in pieces(&samples, rate) {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut bytes, spec).map_err(|e| failed(e.to_string()))?;
        for &sample in &samples[start..end] {
            writer.write_sample(sample).map_err(|e| failed(e.to_string()))?;
        }
        writer.finalize().map_err(|e| failed(e.to_string()))?;
        let transcript = client.transcribe(&model, bytes.get_ref(), "wav", src_lang).map_err(|e| failed(format!("{e:#}")))?;
        let offset = start as f64 / rate as f64;
        if transcript.segments.is_empty() {
            if !transcript.text.is_empty() {
                out.push((offset, end as f64 / rate as f64, transcript.text));
            }
            continue;
        }
        out.extend(transcript.segments.into_iter().map(|s| (offset + s.start, offset + s.end, s.text)));
    }
    if out.is_empty() {
        return Err(t!("cloud-asr-empty"));
    }
    Ok(out)
}

/// Куски записи под один запрос: провайдеры OpenRouter не берут тело больше 25 МБ и обрывают запрос через 60 с,
/// поэтому кусок не длиннее 5 минут, а разрез — в самом тихом месте последних 10 секунд, не посреди слова.
fn pieces(samples: &[i16], rate: usize) -> Vec<(usize, usize)> {
    const PIECE: usize = 300;
    const SEEK: usize = 10;
    let window = rate / 10;
    let mut out = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        let limit = start + PIECE * rate;
        if limit >= samples.len() {
            out.push((start, samples.len()));
            break;
        }
        let quietest = (limit - SEEK * rate..limit - window)
            .step_by(window.max(1))
            .min_by_key(|&at| samples[at..at + window].iter().map(|&s| (s as i64).abs()).sum::<i64>())
            .unwrap_or(limit);
        out.push((start, quietest));
        start = quietest;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_recording_is_cut_in_its_quietest_place_and_a_short_one_is_whole() {
        let rate = 16_000;
        let mut samples = vec![1_000i16; rate * 700];
        let quiet = rate * 295;
        samples[quiet..quiet + rate / 10].fill(0);
        let cuts = pieces(&samples, rate);
        assert_eq!(cuts[0], (0, quiet));
        assert_eq!(cuts.last().unwrap().1, samples.len());
        assert!(cuts.iter().all(|(start, end)| end - start <= 300 * rate));
        assert!(cuts.windows(2).all(|pair| pair[0].1 == pair[1].0));
        assert_eq!(pieces(&samples[..rate * 60], rate), vec![(0, rate * 60)]);
    }
}
