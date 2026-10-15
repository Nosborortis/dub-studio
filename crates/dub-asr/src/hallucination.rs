//! Фильтр галлюцинаций распознавания: фразы, которые ASR пишет поверх музыки и тишины вместо
//! «ничего не услышал» (титры субтитровщиков, «Продолжение следует», «Thanks for watching», звуки в
//! скобках). Список и правила — из студий (YuE2-Studio lyrics_sync.rs), адаптированы под дубляж:
//! сегмент судится целиком, поэтому реплика, лишь содержащая такую фразу, не срабатывает.

/// Фразы, которые Whisper пишет поверх музыки и тишины: дословные галлюцинации исследования
/// «Bag of Hallucinations» (Barański et al., ICASSP 2025, MIT) и поязычные списки конвейера
/// NVIDIA NeMo Granary (Apache-2.0) в сборке Scicom-AI/Whisper-Hallucination; фразы от двух слов,
/// нормализованные так же, как `normalised`. Файл идентичен копиям студий.
const HALLUCINATIONS: &str = include_str!("whisper_hallucinations.txt");

/// Имена и адреса из титров субтитровщиков: в диалоге их не бывает, поэтому они — титр сами по себе.
/// Сравнение идёт по нормализованному тексту, где точка и дефис — разделители («amara org», «altyazı m k»).
const CREDIT_MARKERS: &[&str] = &[
    "dimatorzok",
    "amara org",
    "napisy stworzone",
    "titulky vytvořil",
    "johnyx",
    "altyazı m k",
    "sous titrage st 501",
    "نانسي قنقر",
];

/// Слово «субтитры» на языках дубляжа: части — начала соседних слов нормализованного текста. Слово
/// бывает и в реплике («Включите субтитры», «Mach die Untertitel an»), поэтому титром оно становится
/// только рядом с автором или когда вся фраза есть в списке галлюцинаций; иначе признак слабый.
const SUBTITLE_WORDS: &[&[&str]] = &[
    &["субтитр"],
    &["subtitle"],
    &["untertitel"],
    &["sous", "titr"],
    &["subtítulo"],
    &["sottotitol"],
    &["legendas"],
    &["ondertitel"],
];
/// То же в письменностях без пробелов между словами: ищется подстрокой.
const SUBTITLE_WORDS_UNSPACED: &[&str] = &["字幕", "자막"];
/// Предлог автора за словом «субтитры»: «subtitles by», «sous-titres réalisés par», «subtítulos por»,
/// «legendas pela», «Untertitel von», «ondertitels door».
const AUTHOR_PREPOSITIONS: &[&str] = &["by", "par", "por", "pela", "pelo", "von", "door"];
/// Слово после предлога, с которым тот — оборот, а не автор: «subtítulos, por favor», «par défaut».
const PREPOSITION_IDIOMS: &[&str] = &["favor", "défaut", "defecto", "padrão", "exemple", "ejemplo", "exemplo"];
/// Начала слов об авторстве рядом со словом «субтитры»: «Субтитры сделал …», «Редактор субтитров …»,
/// «Untertitel im Auftrag des ZDF», «sottotitoli a cura di …», сообщество Amara.
const AUTHOR_WORDS: &[&str] =
    &["сделал", "делал", "создавал", "создал", "добавил", "подготов", "редактор", "корректор", "amara", "auftrag", "cura"];
/// То же в письменностях без пробелов: «字幕提供者», «자막제작».
const AUTHOR_WORDS_UNSPACED: &[&str] = &["提供", "制作", "製作", "제작", "제공"];
/// Сколько слов по соседству со словом «субтитры» считается «рядом».
const AUTHOR_WINDOW: usize = 3;

/// Какой набор правил применять к сегменту.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HallucinationRules {
    /// Whisper: полный студийный набор, включая «КРИК» (капсом Whisper пишет звуки: «ВЕСЕЛАЯ МУЗЫКА»)
    /// и сегмент из одних цифр (« 1.»).
    Whisper,
    /// Parakeet и облачный STT: регистр у них настоящий («NASA!» — реплика), числа пишутся цифрами,
    /// поэтому правила крика и одних цифр выключены.
    CaseAware,
}

fn normalised(text: &str) -> String {
    let lowered = text.to_lowercase().replace('ё', "е");
    lowered
        .split(|c: char| c.is_whitespace() || ".,!?…\"'«»“”„-–—:;()[]♪。、！？「」".contains(c))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn known_phrases() -> &'static std::collections::HashSet<&'static str> {
    static KNOWN: std::sync::OnceLock<std::collections::HashSet<&'static str>> = std::sync::OnceLock::new();
    KNOWN.get_or_init(|| HALLUCINATIONS.lines().map(str::trim).filter(|line| !line.is_empty()).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubtitleMention {
    None,
    /// Слово «субтитры» без автора рядом.
    Plain,
    /// Слово «субтитры» с автором рядом — титр.
    Credited,
}

/// Автор рядом со словом «субтитры», занявшим слова `start..=end`: предлог в пределах окна после него
/// (не оборот вроде «por favor») или слово об авторстве в окне с обеих сторон.
fn authored(tokens: &[&str], start: usize, end: usize) -> bool {
    let after_end = tokens.len().min(end + 1 + AUTHOR_WINDOW);
    let by = (end + 1..after_end).any(|i| {
        AUTHOR_PREPOSITIONS.contains(&tokens[i])
            && !tokens.get(i + 1).is_some_and(|next| PREPOSITION_IDIOMS.contains(next))
    });
    by || tokens[start.saturating_sub(AUTHOR_WINDOW)..after_end]
        .iter()
        .any(|tok| AUTHOR_WORDS.iter().any(|w| tok.starts_with(w)))
}

fn subtitle_mention(plain: &str) -> SubtitleMention {
    let mut mention = SubtitleMention::None;
    if SUBTITLE_WORDS_UNSPACED.iter().any(|w| plain.contains(w)) {
        if AUTHOR_WORDS_UNSPACED.iter().any(|w| plain.contains(w)) {
            return SubtitleMention::Credited;
        }
        mention = SubtitleMention::Plain;
    }
    let tokens: Vec<&str> = plain.split(' ').collect();
    for word in SUBTITLE_WORDS {
        for start in 0..tokens.len().saturating_sub(word.len() - 1) {
            let end = start + word.len() - 1;
            if !word.iter().zip(&tokens[start..=end]).all(|(part, tok)| tok.starts_with(part)) {
                continue;
            }
            if authored(&tokens, start, end) {
                return SubtitleMention::Credited;
            }
            mention = SubtitleMention::Plain;
        }
    }
    mention
}

/// Почему сегмент похож на галлюцинацию.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HallucinationKind {
    /// Ни букв, ни цифр («...», «♪ ♪»).
    Wordless,
    /// Одни цифры без букв (только Whisper): « 1.» в тишине — заполнение паузы, но « 300.» — реплика
    /// «Триста.», числа Whisper пишет цифрами.
    Numeric,
    /// Звук, записанный как субтитр: «[Music]», «(laughs)», «♪».
    Bracketed,
    /// Титр субтитровщика («Субтитры сделал DimaTorzok», «Amara.org»).
    Credit,
    /// Слово «субтитры» без автора: так звучат обрывки титров, но и реплика («Включите субтитры»).
    SubtitleWord,
    /// Крик капсом (только Whisper): так Whisper пишет звуки — «ВЕСЕЛАЯ МУЗЫКА».
    Shouted,
    /// Фраза из списка целиком. Среди них и обычные реплики диалога («come on», «thank you»,
    /// «watch out»): Whisper пишет их в тишине, но люди их и правда говорят.
    KnownPhrase,
    /// Строка зациклена внутри: разных слов не больше 0,4 при пяти и больше словах (критерий NeMo SDP).
    Looped,
    /// Слово длиннее 25 букв — слипшийся мусор распознавания.
    LongWord,
    /// Та же строка от трёх слов сразу после себя или три раза и больше в ролике.
    RepeatedLine,
}

impl HallucinationKind {
    /// Признак почти не встречается в настоящей речи — годится для решения без свидетельства голоса.
    pub fn is_strong(self) -> bool {
        matches!(self, HallucinationKind::Wordless | HallucinationKind::Bracketed | HallucinationKind::Credit)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HallucinationKind::Wordless => "wordless",
            HallucinationKind::Numeric => "numeric",
            HallucinationKind::Bracketed => "bracketed",
            HallucinationKind::Credit => "credit",
            HallucinationKind::SubtitleWord => "subtitle_word",
            HallucinationKind::Shouted => "shouted",
            HallucinationKind::KnownPhrase => "known_phrase",
            HallucinationKind::Looped => "looped",
            HallucinationKind::LongWord => "long_word",
            HallucinationKind::RepeatedLine => "repeated_line",
        }
    }
}

/// Признаки зацикливания распознавания, которые включаются по отдельности.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoopRules {
    /// Повтор внутри строки.
    pub inside: bool,
    /// Повтор строки подряд или по ролику.
    pub lines: bool,
    /// Слово длиннее 25 букв.
    pub long_words: bool,
}

const LOOP_MIN_WORDS: usize = 5;
const LOOP_MAX_DISTINCT: f64 = 0.4;
const LONG_WORD_LETTERS: usize = 25;
const REPEATED_LINE_MIN_WORDS: usize = 3;
const REPEATED_LINE_TIMES: usize = 3;

/// Признак зацикливания самой строки; None — нет.
pub fn loop_kind(text: &str, rules: LoopRules) -> Option<HallucinationKind> {
    let words: Vec<String> = normalised(text).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect();
    if rules.inside && words.len() >= LOOP_MIN_WORDS {
        let distinct = words.iter().collect::<std::collections::HashSet<_>>().len();
        if distinct as f64 / words.len() as f64 <= LOOP_MAX_DISTINCT {
            return Some(HallucinationKind::Looped);
        }
    }
    if rules.long_words && text.split(|c: char| !c.is_alphabetic()).any(|w| w.chars().count() > LONG_WORD_LETTERS) {
        return Some(HallucinationKind::LongWord);
    }
    None
}

/// Какие строки повторяют строку перед собой или звучат три раза и больше; строки короче трёх слов не
/// судятся — короткие реплики («Да.», «Что?») в речи повторяются по-настоящему.
pub fn repeated_lines(texts: &[&str]) -> Vec<bool> {
    let keys: Vec<Option<String>> = texts
        .iter()
        .map(|t| {
            let n = normalised(t);
            (n.split(' ').filter(|w| !w.is_empty()).count() >= REPEATED_LINE_MIN_WORDS).then_some(n)
        })
        .collect();
    let mut times: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for k in keys.iter().flatten() {
        *times.entry(k.as_str()).or_insert(0) += 1;
    }
    keys.iter()
        .enumerate()
        .map(|(i, k)| {
            k.as_ref().is_some_and(|k| {
                times[k.as_str()] >= REPEATED_LINE_TIMES || (i > 0 && keys[i - 1].as_deref() == Some(k.as_str()))
            })
        })
        .collect()
}

/// Почему распознанный сегмент — заполнение паузы, а не сказанные слова; None — обычная реплика.
pub fn hallucination_kind(text: &str, rules: HallucinationRules) -> Option<HallucinationKind> {
    let trimmed = text.trim();
    if !trimmed.chars().any(char::is_alphanumeric) {
        return Some(HallucinationKind::Wordless);
    }
    if !trimmed.chars().any(char::is_alphabetic) {
        return (rules == HallucinationRules::Whisper).then_some(HallucinationKind::Numeric);
    }
    let bracketed = (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('(') && trimmed.ends_with(')'))
        || trimmed.starts_with('♪');
    if bracketed {
        return Some(HallucinationKind::Bracketed);
    }
    let plain = normalised(trimmed);
    if CREDIT_MARKERS.iter().any(|marker| plain.contains(marker)) {
        return Some(HallucinationKind::Credit);
    }
    let known = known_phrases().contains(plain.as_str());
    match subtitle_mention(&plain) {
        SubtitleMention::Credited => return Some(HallucinationKind::Credit),
        SubtitleMention::Plain if known => return Some(HallucinationKind::Credit),
        SubtitleMention::Plain => return Some(HallucinationKind::SubtitleWord),
        SubtitleMention::None => {}
    }
    if rules == HallucinationRules::Whisper {
        let letters: Vec<char> = trimmed.chars().filter(|c| c.is_alphabetic()).collect();
        let shouted = letters.len() >= 3
            && letters.iter().all(|c| !c.is_lowercase())
            && letters.iter().any(|c| c.is_uppercase());
        if shouted {
            return Some(HallucinationKind::Shouted);
        }
    }
    known.then_some(HallucinationKind::KnownPhrase)
}

/// Является ли распознанный сегмент заполнением паузы, а не сказанными словами (см. HallucinationKind).
pub fn is_hallucination(text: &str, rules: HallucinationRules) -> bool {
    hallucination_kind(text, rules).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: HallucinationRules = HallucinationRules::Whisper;
    const C: HallucinationRules = HallucinationRules::CaseAware;

    #[test]
    fn whisper_fillers_are_dropped_and_real_lines_kept() {
        assert!(is_hallucination(" Субтитры создавал DimaTorzok", W));
        assert!(is_hallucination("Продолжение следует...", W));
        assert!(is_hallucination("Thanks for watching!", W));
        assert!(is_hallucination("ご視聴ありがとうございました", W));
        assert!(is_hallucination("ВЕСЕЛАЯ МУЗЫКА", W));
        assert!(is_hallucination("[Music]", W));
        assert!(is_hallucination(" 1.", W));
        assert!(!is_hallucination("Если б мне платили каждый раз,", W));
        assert!(!is_hallucination("Спасибо, что ты рядом со мной", W));
        assert!(!is_hallucination("Поехали!", W));
    }

    #[test]
    fn credits_in_the_dub_languages_are_caught() {
        let credit = |t: &str| hallucination_kind(t, C) == Some(HallucinationKind::Credit);
        assert!(credit("Subtitles by the Amara.org community"));
        assert!(credit("Sous-titres réalisés par la communauté d'Amara.org"));
        assert!(credit("Napisy stworzone przez społeczność Amara.org"));
        assert!(credit("Ondertitels ingediend door de Amara.org gemeenschap"));
        assert!(credit("Titulky vytvořil JohnyX"));
        assert!(credit("Altyazı M.K."));
        assert!(credit("ترجمة نانسي قنقر"));
        assert!(credit("Untertitel im Auftrag des ZDF, 2021"));
        assert!(credit("Sous-titrage ST' 501"));
    }

    #[test]
    fn case_aware_rules_keep_shouting_and_numbers() {
        assert!(!is_hallucination("NASA!", C));
        assert!(!is_hallucination("ПОМОГИТЕ!", C));
        assert!(!is_hallucination("2024.", C));
        assert!(is_hallucination("...", C));
        assert!(is_hallucination("(laughs)", C));
    }

    #[test]
    fn the_kind_says_how_sure_the_text_alone_is() {
        let k = |t: &str, r| hallucination_kind(t, r);
        assert_eq!(k("Субтитры сделал DimaTorzok", W), Some(HallucinationKind::Credit));
        assert_eq!(k("[Music]", C), Some(HallucinationKind::Bracketed));
        assert_eq!(k("...", C), Some(HallucinationKind::Wordless));
        assert_eq!(k("ВЕСЕЛАЯ МУЗЫКА", W), Some(HallucinationKind::Shouted));
        assert_eq!(k("Watch out!", C), Some(HallucinationKind::KnownPhrase));
        assert!(HallucinationKind::Credit.is_strong());
        assert!(!HallucinationKind::KnownPhrase.is_strong());
        assert!(!HallucinationKind::Shouted.is_strong());
    }

    #[test]
    fn whisper_numbers_without_letters_are_a_weak_sign() {
        let k = |t: &str, r| hallucination_kind(t, r);
        assert_eq!(k(" 1.", W), Some(HallucinationKind::Numeric));
        assert_eq!(k(" 300.", W), Some(HallucinationKind::Numeric));
        assert_eq!(k("10, 9, 8", W), Some(HallucinationKind::Numeric));
        assert!(!HallucinationKind::Numeric.is_strong());
        assert_eq!(k("...", W), Some(HallucinationKind::Wordless));
        assert_eq!(k("♪ ♪", W), Some(HallucinationKind::Wordless));
        assert_eq!(k("2024!", C), None);
    }

    #[test]
    fn the_word_subtitles_is_a_credit_only_next_to_its_author() {
        let k = |t: &str| hallucination_kind(t, C);
        for credit in [
            "Субтитры сделал Вася Пупкин",
            "Редактор субтитров А.Семкин",
            "Подпишись на канал, субтитры",
            "Subtítulos por la comunidad",
            "Legendas pela equipe Fansub",
            "Untertitel von Stephanie Geiges",
            "Sottotitoli a cura di QTSS",
            "Sous-titrage Société Radio-Canada",
            "字幕提供者 李宗盛",
        ] {
            assert_eq!(k(credit), Some(HallucinationKind::Credit), "{credit}");
        }
        for line in [
            "Включите субтитры",
            "Mach die Untertitel an",
            "Pon los subtítulos, por favor",
            "Gracias por los subtítulos",
            "Mets les sous-titres par défaut",
            "Ative as legendas",
            "Turn on the subtitles",
            "打开字幕",
        ] {
            assert_eq!(k(line), Some(HallucinationKind::SubtitleWord), "{line}");
        }
        assert!(!HallucinationKind::SubtitleWord.is_strong());
        assert_eq!(k("That was legendary"), None);
    }

    #[test]
    fn a_line_that_only_contains_a_known_phrase_stays() {
        assert!(is_hallucination("Thank you for watching.", C));
        assert!(!is_hallucination("Thank you for watching my brother while I was away.", C));
    }
}
