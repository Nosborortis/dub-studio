//! Проверки конвейера, которые включает и выключает пользователь (слоты active.json, раздел «Качество»).
//! Подпункт действует, только пока включён его пункт.

use serde_json::Value;

/// (ключ, пункт-родитель, включён по умолчанию).
pub const SWITCHES: &[(&str, Option<&str>, bool)] = &[
    ("asr_repeats", None, true),
    ("asr_repeat_inside", Some("asr_repeats"), true),
    ("asr_repeat_lines", Some("asr_repeats"), true),
    ("asr_long_words", Some("asr_repeats"), true),
    ("tr_gender", None, true),
    ("tr_gender_check", Some("tr_gender"), true),
    ("tr_latin", None, true),
    ("tts_caps", None, true),
    ("tts_symbols", None, true),
    ("qc_audio", None, true),
    ("qc_repeat", Some("qc_audio"), true),
    ("qc_end", Some("qc_audio"), true),
    ("qc_silence", Some("qc_audio"), true),
    ("qc_cloud", Some("qc_audio"), true),
    ("qc_asr", None, false),
    ("qc_asr_letters", Some("qc_asr"), true),
    ("qc_asr_repeat", Some("qc_asr"), true),
    ("qc_voice_gender", None, true),
];

pub fn is_switch(key: &str) -> bool {
    SWITCHES.iter().any(|(k, _, _)| *k == key)
}

/// Действует ли проверка `key` при выборе `sel`.
pub fn on(sel: &Value, key: &str) -> bool {
    let Some(&(_, parent, default)) = SWITCHES.iter().find(|(k, _, _)| *k == key) else {
        panic!("unknown check switch {key}");
    };
    let own = match sel.get(key).and_then(Value::as_str) {
        Some(v) if default => v != "0",
        Some(v) => v == "1",
        None => default,
    };
    own && parent.is_none_or(|p| on(sel, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_sub_check_follows_its_parent_and_defaults_apply_to_absent_slots() {
        let none = json!({});
        assert!(on(&none, "qc_repeat"));
        assert!(!on(&none, "qc_asr"));
        assert!(!on(&none, "qc_asr_letters"), "the letters check waits for the ASR check");
        assert!(on(&json!({ "qc_asr": "1" }), "qc_asr_letters"));
        assert!(!on(&json!({ "qc_audio": "0" }), "qc_end"));
        assert!(!on(&json!({ "qc_end": "0" }), "qc_end"));
        assert!(on(&json!({ "qc_end": "0" }), "qc_repeat"));
    }

    #[test]
    fn every_parent_is_a_switch_listed_before_its_children() {
        for (i, (k, parent, _)) in SWITCHES.iter().enumerate() {
            if let Some(p) = parent {
                let at = SWITCHES.iter().position(|(x, _, _)| x == p).unwrap_or_else(|| panic!("{k}: no parent {p}"));
                assert!(at < i, "{k} comes after {p}");
            }
        }
    }
}
