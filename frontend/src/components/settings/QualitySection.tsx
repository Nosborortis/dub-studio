import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AudioLines, AudioWaveform, Captions, CaseUpper, Clock, CloudAlert, Ear, Hash, Languages, Mic2, Repeat, Repeat2, Rewind,
  Scissors, Sparkles, SpellCheck, Star, Timer, VenusAndMars, VolumeX, WholeWord, type LucideIcon } from "lucide-react";
import { api } from "../../lib/api";
import SettingSwitch from "./SettingSwitch";
import { useChanged } from "../editorBridge";
import { SETTINGS_CHANGED } from "../../lib/mcpBridge";

// Тумблеры качества — слоты active.json, по стадиям конвейера. defaultOn: слот, которого ещё нет в файле, считается
// включённым. parent: подпункт действует, только пока включён его пункт (так же решает сервер, checks.rs).
type Row = { key: string; icon: LucideIcon; label: string; defaultOn: boolean; parent?: string };

const row = (key: string, icon: LucideIcon, label: string, defaultOn: boolean, parent?: string): Row => ({ key, icon, label, defaultOn, parent });

const RECOGNITION: Row[] = [
  row("asr_repeats", Repeat, "qc.asrRepeats", true),
  row("asr_repeat_inside", Repeat2, "qc.asrRepeatInside", true, "asr_repeats"),
  row("asr_repeat_lines", Repeat, "qc.asrRepeatLines", true, "asr_repeats"),
  row("asr_long_words", WholeWord, "qc.asrLongWords", true, "asr_repeats"),
];

const TRANSLATION: Row[] = [
  row("tr_gender", VenusAndMars, "qc.trGender", true),
  row("tr_gender_check", SpellCheck, "qc.trGenderCheck", true, "tr_gender"),
  row("tr_latin", Languages, "qc.trLatin", true),
];

const VOICE: Row[] = [
  row("speech_rate_on", Sparkles, "qc.speechRate", true),
  row("qc_duration", Clock, "qc.duration", true),
  row("lead_into_silence", Rewind, "qc.leadIntoSilence", false),
  row("auto_shorten", Scissors, "qc.autoShorten", true),
  row("emo_ref_on", Mic2, "qc.emoRef", true),
  row("breath_on", AudioLines, "qc.breath", false),
  row("multitake", Star, "qc.multitake", false),
  row("tts_caps", CaseUpper, "qc.ttsCaps", true),
  row("tts_symbols", Hash, "qc.ttsSymbols", true),
];

const TAKES: Row[] = [
  row("qc_audio", AudioWaveform, "qc.audio", true),
  row("qc_repeat", Repeat2, "qc.audioRepeat", true, "qc_audio"),
  row("qc_end", AudioWaveform, "qc.audioEnd", true, "qc_audio"),
  row("qc_silence", VolumeX, "qc.audioSilence", true, "qc_audio"),
  row("qc_cloud", CloudAlert, "qc.audioCloud", true, "qc_audio"),
  row("qc_asr", Captions, "qc.asr", false),
  row("qc_asr_letters", SpellCheck, "qc.asrLetters", true, "qc_asr"),
  row("qc_asr_repeat", Ear, "qc.asrRepeat", true, "qc_asr"),
  row("qc_voice_gender", VenusAndMars, "qc.voiceGender", true),
];

const DIAGNOSTICS: Row[] = [row("bench", Timer, "settings.bench", false)];

const ALL = [...RECOGNITION, ...TRANSLATION, ...VOICE, ...TAKES, ...DIAGNOSTICS];

type Selection = Record<string, unknown>;

const ownOn = (sel: Selection, r: Row) => (r.defaultOn ? sel[r.key] !== "0" : sel[r.key] === "1");

const isOn = (sel: Selection, r: Row): boolean => {
  const parent = r.parent ? ALL.find((p) => p.key === r.parent) : undefined;
  return ownOn(sel, r) && (!parent || isOn(sel, parent));
};

const texts = (t: (k: string) => string, r: Row) =>
  r.key === "bench" ? { label: t("settings.bench"), hint: t("settings.benchHint"), tip: t("settings.benchHint") }
    : { label: t(`${r.label}.label`), hint: t(`${r.label}.hint`), tip: t(`${r.label}.tip`) };

export default function QualitySection() {
  const { t } = useTranslation();
  const [sel, setSel] = useState<Selection | null>(null);
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reread, setReread] = useState(0);
  useChanged(SETTINGS_CHANGED, () => setReread((n) => n + 1));

  useEffect(() => {
    api.capabilities()
      .then((c) => setSel(c.selection ?? {}))
      .catch((e: unknown) => setError(t("prefs.loadFailed", { error: e instanceof Error ? e.message : String(e) })));
  }, [t, reread]);

  const toggle = async (r: Row) => {
    if (!sel || saving) return;
    const next = ownOn(sel, r) ? "0" : "1";
    setSaving(r.key);
    setError(null);
    try {
      setSel(await api.setSelection(r.key, next));
    } catch (e) {
      setError(t("prefs.saveFailed", { error: e instanceof Error ? e.message : String(e) }));
    } finally {
      setSaving(null);
    }
  };

  const group = (part: string, title: string, rows: readonly Row[]) => (
    <section className="mb-5" data-settings-part={part}>
      <h4 className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)] mb-2">{title}</h4>
      <div className="rounded-lg border border-[var(--color-border)] divide-y divide-[var(--color-border)] bg-[var(--color-surface-2)]/40">
        {rows.map((r) => {
          const parent = r.parent ? ALL.find((p) => p.key === r.parent) : undefined;
          const parentOff = !!(sel && parent && !isOn(sel, parent));
          const { label, hint, tip } = texts(t, r);
          return (
            <div key={r.key} className={parent ? "pl-6" : undefined}>
              <SettingSwitch icon={r.icon} label={label} hint={hint} tip={tip}
                on={sel ? ownOn(sel, r) && !parentOff : r.defaultOn} busy={saving === r.key}
                disabled={!sel || saving !== null || parentOff}
                onToggle={() => { void toggle(r); }} />
            </div>
          );
        })}
      </div>
    </section>
  );

  return (
    <div className="max-w-2xl">
      {error && <p role="alert" className="mb-3 mono text-[11px] text-[var(--color-warn)] break-words">{error}</p>}
      {group("recognition", t("prefs.recognitionGroup"), RECOGNITION)}
      {group("translation", t("prefs.translationGroup"), TRANSLATION)}
      {group("voice", t("prefs.voiceGroup"), VOICE)}
      {group("takes", t("prefs.takesGroup"), TAKES)}
      {group("diagnostics", t("prefs.diagnostics"), DIAGNOSTICS)}
    </div>
  );
}
