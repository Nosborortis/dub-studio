//! Признаки брака готового дубля по его звуку, один проход по сэмплам: клиппинг, доля речи, длинная
//! пауза, громкий конец клипа (обрыв на полуслове), шум в паузах, тональный свист и повтор куска фразы
//! по матрице самоподобия лог-мел (Foster, Klapuri, Dixon, EUSIPCO 2012). Пороги приёмки — в `flags`.

use realfft::RealFftPlanner;

/// Кадр огибающей, с.
const FRAME_SECS: f64 = 0.010;
/// Кадр громче пика огибающей минус столько дБ — речь.
const SPEECH_BELOW_PEAK_DB: f64 = 35.0;
const NFFT: usize = 1024;
const MEL_BANDS: usize = 40;
/// Кадры лог-мел, похожие на кадр со сдвигом не меньше `REPEAT_MIN_LAG_SECS`, — повтор.
const REPEAT_COSINE: f64 = 0.93;
const REPEAT_MIN_LAG_SECS: f64 = 0.4;
/// Повтор и свист ищутся в первых секундах клипа: матрица самоподобия квадратична по длине.
const SPECTRAL_MAX_SECS: f64 = 60.0;

#[derive(Debug, Clone, Default)]
pub struct Audit {
    /// Длина клипа, с.
    pub secs: f64,
    /// Доля кадров речи.
    pub speech_frac: f64,
    /// Событий клиппинга: три сэмпла подряд на пределе шкалы.
    pub clip_events: usize,
    /// Самая длинная тишина между первым и последним кадром речи, с.
    pub max_pause_secs: f64,
    /// Пик последних 20 мс относительно пика клипа, дБ.
    pub end_edge_db: f64,
    /// Медиана уровня кадров-пауз внутри речи, дБ; None — пауз нет.
    pub noise_db: Option<f64>,
    /// Доля кадров речи с одним и тем же узким тоном 2–10 кГц на 30 дБ выше медианы спектра.
    pub tonal_frac: f64,
    /// Самая длинная полоса повтора матрицы самоподобия, с.
    pub repeat_secs: f64,
    /// От первого до последнего кадра речи, с.
    pub voiced_secs: f64,
}

/// Какие признаки судить: `repeat`, `end` и `silence` — у любого синтеза, `cloud` — болезни облачных моделей.
#[derive(Debug, Clone, Copy, Default)]
pub struct Judge {
    pub repeat: bool,
    pub end: bool,
    pub silence: bool,
    pub cloud: bool,
}

impl Judge {
    pub fn any(&self) -> bool {
        self.repeat || self.end || self.silence || self.cloud
    }
}

const VOWELS: &str = "aeiouyàáâãäåèéêëìíîïòóôõöùúûüýæœаеёиоуыэюяіїє";

fn syllables(text: &str) -> usize {
    text.chars().flat_map(char::to_lowercase).filter(|c| VOWELS.contains(*c)).count()
}

/// В тексте одно слово (основа — первые 4 буквы слова от 3 букв) звучит не раз: повтор в звуке
/// тогда может быть сказанным, и по звуку его не судим.
fn text_repeats(text: &str) -> bool {
    let mut seen = std::collections::HashSet::new();
    text.split(|c: char| !c.is_alphabetic())
        .filter(|w| w.chars().count() >= 3)
        .map(|w| w.to_lowercase().replace('ё', "е").chars().take(4).collect::<String>())
        .any(|stem| !seen.insert(stem))
}

/// Признак брака дубля по его звуку и тексту; None — признаков нет.
pub fn defect(a: &Audit, text: &str, j: Judge) -> Option<&'static str> {
    if j.end && a.secs >= 1.0 && a.end_edge_db >= -3.0 {
        return Some("end_burst");
    }
    if j.repeat && a.repeat_secs >= 0.2 && syllables(text) >= 4 && !text_repeats(text) {
        return Some("repeat");
    }
    if j.silence && a.secs >= 3.0 && a.speech_frac < 0.3 {
        return Some("padding");
    }
    if j.cloud {
        if a.voiced_secs >= 3.0 && a.tonal_frac >= 0.3 {
            return Some("whistle");
        }
        if a.max_pause_secs >= 0.25 && a.noise_db.is_some_and(|n| n >= -45.0) {
            return Some("hiss");
        }
    }
    None
}

/// Признак этого модуля (а не базовой проверки синтеза render::synth_defect).
pub fn is_take_sign(kind: &str) -> bool {
    matches!(kind, "end_burst" | "repeat" | "padding" | "whistle" | "hiss")
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-9).log10()
}

/// Треугольный мел-банк: для каждой полосы — (бин, вес).
fn mel_bank(sr: f64, nfft: usize, bands: usize) -> Vec<Vec<(usize, f64)>> {
    let to_mel = |f: f64| 2595.0 * (1.0 + f / 700.0).log10();
    let from_mel = |m: f64| 700.0 * (10f64.powf(m / 2595.0) - 1.0);
    let (lo, hi) = (to_mel(80.0), to_mel((sr / 2.0).min(8000.0)));
    let pts: Vec<f64> = (0..bands + 2).map(|i| from_mel(lo + (hi - lo) * i as f64 / (bands + 1) as f64)).collect();
    let bin = |f: f64| f * nfft as f64 / sr;
    (0..bands)
        .map(|b| {
            let (l, c, r) = (bin(pts[b]), bin(pts[b + 1]), bin(pts[b + 2]));
            ((l.floor() as usize)..=(r.ceil() as usize).min(nfft / 2))
                .filter_map(|k| {
                    let k_f = k as f64;
                    let w = if k_f < c { (k_f - l) / (c - l) } else { (r - k_f) / (r - c) };
                    (w > 0.0).then_some((k, w))
                })
                .collect()
        })
        .collect()
}

/// Самая длинная диагональ подряд совпавших кадров со сдвигом не меньше `min_lag`, в кадрах.
fn longest_repeat(frames: &[Option<Vec<f64>>], min_lag: usize) -> usize {
    let n = frames.len();
    let mut best = 0usize;
    for lag in min_lag..n {
        let mut run = 0usize;
        for i in 0..n - lag {
            let same = match (&frames[i], &frames[i + lag]) {
                (Some(a), Some(b)) => a.iter().zip(b).map(|(p, q)| p * q).sum::<f64>() >= REPEAT_COSINE,
                _ => false,
            };
            if same {
                run += 1;
                best = best.max(run);
            } else {
                run = 0;
            }
        }
    }
    best
}

pub fn audit(x: &[f32], sr: u32) -> Audit {
    let mut out = Audit { secs: x.len() as f64 / sr.max(1) as f64, ..Audit::default() };
    if sr == 0 || x.len() < NFFT {
        return out;
    }
    let mut run = 0usize;
    for v in x {
        if v.abs() >= 0.999 {
            run += 1;
            if run == 3 {
                out.clip_events += 1;
            }
        } else {
            run = 0;
        }
    }

    let w = ((sr as f64 * FRAME_SECS) as usize).max(1);
    let rms: Vec<f64> = x.chunks(w).map(|c| (c.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / c.len() as f64).sqrt()).collect();
    let peak = rms.iter().cloned().fold(0.0f64, f64::max);
    let thr = peak * 10f64.powf(-SPEECH_BELOW_PEAK_DB / 20.0);
    let loud: Vec<bool> = rms.iter().map(|&v| v >= thr && v > 1e-6).collect();
    out.speech_frac = loud.iter().filter(|&&l| l).count() as f64 / loud.len().max(1) as f64;
    let mut quiet_frames: Vec<usize> = Vec::new();
    if let (Some(a), Some(b)) = (loud.iter().position(|&l| l), loud.iter().rposition(|&l| l)) {
        let (mut q, mut longest) = (0usize, 0usize);
        for (i, &l) in loud.iter().enumerate().take(b + 1).skip(a) {
            if l {
                q = 0;
            } else {
                q += 1;
                longest = longest.max(q);
                quiet_frames.push(i);
            }
        }
        out.max_pause_secs = longest as f64 * w as f64 / sr as f64;
        out.voiced_secs = (b + 1 - a) as f64 * w as f64 / sr as f64;
    }
    let abs_peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    let n20 = (sr as usize / 50).min(x.len());
    let end_peak = x[x.len() - n20..].iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    out.end_edge_db = db(end_peak / abs_peak.max(1e-9));
    let mut qr: Vec<f64> = quiet_frames.iter().map(|&i| rms[i]).collect();
    qr.sort_by(|a, b| a.total_cmp(b));
    out.noise_db = (!qr.is_empty()).then(|| db(qr[qr.len() / 2]));

    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(NFFT);
    let hann: Vec<f64> = (0..NFFT).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / NFFT as f64).cos()).collect();
    let mut buf = fft.make_input_vec();
    let mut spec = fft.make_output_vec();
    let hop = NFFT / 2;
    let (lo, hi) = (
        ((2000.0 * NFFT as f64) / sr as f64) as usize,
        ((10000.0 * NFFT as f64) / sr as f64).min(NFFT as f64 / 2.0 - 1.0) as usize,
    );
    let mel = mel_bank(sr as f64, NFFT, MEL_BANDS);
    let mut tone_hits: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut loud_n = 0usize;
    let mut frames: Vec<Option<Vec<f64>>> = Vec::new();
    let limit = x.len().min((sr as f64 * SPECTRAL_MAX_SECS) as usize);
    let mut start = 0usize;
    while start + NFFT <= limit {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = x[start + i] as f64 * hann[i];
        }
        if fft.process(&mut buf, &mut spec).is_err() {
            break;
        }
        let p: Vec<f64> = spec.iter().map(|c| c.norm_sqr()).collect();
        let is_loud = loud.get((start + NFFT / 2) / w).copied().unwrap_or(false);
        if is_loud && hi > lo {
            loud_n += 1;
            let band = &p[lo..=hi];
            let mut sorted = band.to_vec();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let med = sorted[sorted.len() / 2].max(1e-12);
            let (bi, bv) = band.iter().enumerate().fold((0, 0.0f64), |m, (i, &v)| if v > m.1 { (i, v) } else { m });
            if 10.0 * (bv / med).log10() >= 30.0 {
                *tone_hits.entry(bi / 2).or_default() += 1;
            }
        }
        frames.push(is_loud.then(|| {
            let m: Vec<f64> = mel.iter().map(|f| f.iter().map(|&(b, wt)| p[b] * wt).sum::<f64>().max(1e-10).ln()).collect();
            let mean = m.iter().sum::<f64>() / m.len() as f64;
            let v: Vec<f64> = m.iter().map(|a| a - mean).collect();
            let norm = v.iter().map(|a| a * a).sum::<f64>().sqrt();
            if norm > 1e-9 { v.iter().map(|a| a / norm).collect() } else { Vec::new() }
        }).filter(|v| !v.is_empty()));
        start += hop;
    }
    out.tonal_frac = if loud_n > 0 { tone_hits.values().copied().max().unwrap_or(0) as f64 / loud_n as f64 } else { 0.0 };
    let frame_s = hop as f64 / sr as f64;
    out.repeat_secs = longest_repeat(&frames, (REPEAT_MIN_LAG_SECS / frame_s).ceil() as usize) as f64 * frame_s;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 24_000;

    /// «Слово»: гармоники с плывущим тоном от `f0`, огибающая с атакой и спадом.
    fn word(f0: f64, secs: f64) -> Vec<f32> {
        let n = (secs * SR as f64) as usize;
        (0..n)
            .map(|i| {
                let t = i as f64 / SR as f64;
                let f = f0 + 200.0 * t / secs;
                let env = (t / 0.03).min(1.0) * ((secs - t) / 0.08).clamp(0.0, 1.0);
                let s: f64 = (1..=6).map(|k| (2.0 * std::f64::consts::PI * f * k as f64 * t).sin() / k as f64).sum();
                (0.3 * env * s) as f32
            })
            .collect()
    }

    fn silence(secs: f64) -> Vec<f32> {
        vec![0.0; (secs * SR as f64) as usize]
    }

    const ALL: Judge = Judge { repeat: true, end: true, silence: true, cloud: true };

    #[test]
    fn a_phrase_sounding_twice_is_a_repeat_unless_the_text_repeats_itself() {
        let w = word(140.0, 0.6);
        let x = [w.clone(), silence(0.15), w].concat();
        let a = audit(&x, SR);
        assert!(a.repeat_secs >= 0.2, "{a:?}");
        assert_eq!(defect(&a, "Пошли домой сейчас", ALL), Some("repeat"));
        assert_eq!(defect(&a, "Домой, домой!", ALL), None);
        assert_eq!(defect(&a, "Да.", ALL), None, "too short to judge");
    }

    #[test]
    fn different_sounds_are_not_a_repeat() {
        let mut seed = 12345u32;
        let noise: Vec<f32> = (0..(0.5 * SR as f64) as usize)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 * 0.4 - 0.2
            })
            .collect();
        let tone: Vec<f32> = (0..(0.5 * SR as f64) as usize).map(|i| (0.3 * (2.0 * std::f64::consts::PI * 3000.0 * i as f64 / SR as f64).sin()) as f32).collect();
        let x = [word(120.0, 0.5), silence(0.1), noise, silence(0.1), tone].concat();
        let a = audit(&x, SR);
        assert_eq!(defect(&a, "Пошли домой сейчас", Judge { cloud: false, ..ALL }), None, "{a:?}");
    }

    #[test]
    fn a_loud_last_moment_is_a_burst_and_a_fade_is_not() {
        let mut x = [word(140.0, 1.2), silence(0.2)].concat();
        assert_eq!(defect(&audit(&x, SR), "Пошли домой", ALL), None);
        let n = x.len();
        for (i, v) in x[n - 400..].iter_mut().enumerate() {
            *v = if i % 2 == 0 { 0.95 } else { -0.95 };
        }
        assert_eq!(defect(&audit(&x, SR), "Пошли домой", ALL), Some("end_burst"));
        assert_eq!(defect(&audit(&x, SR), "Пошли домой", Judge { end: false, ..ALL }), None);
    }

    #[test]
    fn a_take_of_mostly_silence_is_judged_when_its_check_is_on() {
        let x = [word(140.0, 0.8), silence(4.0)].concat();
        let a = audit(&x, SR);
        assert_eq!(defect(&a, "Пошли", ALL), Some("padding"));
        assert_eq!(defect(&a, "Пошли", Judge { silence: false, ..ALL }), None);
    }

    /// Прогон по готовым дублям: DUB_AUDIT_DIRS — папки проектов через «;», отчёт в DUB_AUDIT_OUT (TSV).
    #[test]
    #[ignore]
    fn audit_corpus() {
        let dirs = std::env::var("DUB_AUDIT_DIRS").expect("DUB_AUDIT_DIRS");
        let out_path = std::env::var("DUB_AUDIT_OUT").expect("DUB_AUDIT_OUT");
        let mut rows = vec!["file\tsecs\tspeech_frac\tclip\tmax_pause\tend_db\tnoise_db\ttonal\trepeat\ttext\tdefect".to_string()];
        let started = std::time::Instant::now();
        let mut n = 0usize;
        for d in dirs.split(';').filter(|d| !d.is_empty()) {
            let d = std::path::Path::new(d);
            let mut clips: Vec<(std::path::PathBuf, String)> = Vec::new();
            if let Ok(p) = std::fs::read_to_string(d.join("project.json")) {
                let v: serde_json::Value = serde_json::from_str(&p).unwrap();
                for s in v["segments"].as_array().into_iter().flatten() {
                    let id = s["id"].as_str().unwrap_or_default();
                    let f = d.join(format!("seg_{id}.wav"));
                    if f.is_file() {
                        clips.push((f, s["tgt_text"].as_str().unwrap_or_default().to_string()));
                    }
                }
            }
            for e in std::fs::read_dir(d.join("takes")).into_iter().flatten().flatten() {
                let Ok(t) = std::fs::read_to_string(e.path().join("takes.json")) else { continue };
                let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                for k in v["takes"].as_array().into_iter().flatten() {
                    let f = e.path().join(format!("{}.wav", k["n"]));
                    if f.is_file() {
                        clips.push((f, k["text"].as_str().unwrap_or_default().to_string()));
                    }
                }
            }
            for (f, text) in clips {
                let (x, sr) = crate::wavio::read_mono_f32(&f).unwrap();
                let a = audit(&x, sr);
                n += 1;
                let d = defect(&a, &text, Judge { repeat: true, end: true, silence: true, cloud: true }).unwrap_or_default();
                rows.push(format!(
                    "{}\t{:.2}\t{:.3}\t{}\t{:.2}\t{:.1}\t{}\t{:.3}\t{:.2}\t{}\t{d}",
                    f.display(), a.secs, a.speech_frac, a.clip_events, a.max_pause_secs, a.end_edge_db,
                    a.noise_db.map(|v| format!("{v:.1}")).unwrap_or_default(), a.tonal_frac, a.repeat_secs, text
                ));
            }
        }
        std::fs::write(&out_path, rows.join("\n") + "\n").unwrap();
        eprintln!("[OK] clips {n} in {:.1}s", started.elapsed().as_secs_f64());
    }
}
