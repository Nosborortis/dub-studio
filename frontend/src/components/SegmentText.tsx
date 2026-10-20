import { useState } from "react";
import { useTranslation } from "react-i18next";
import { BookA, Check, Loader2, Tag as TagIcon, VolumeX, X } from "lucide-react";
import { api, type Segment, type TtsSkip } from "../lib/api";
import { wordSwap } from "../lib/wordSwap";
import { useStore } from "../store";

// Распознанный текст фразы в редакторе: правится на месте; замена слова предлагает записать термин в глоссарий.
export function SourceText({ pid, seg }: { pid: string; seg: Segment }) {
  const { t } = useTranslation();
  const project = useStore((s) => s.project);
  const setProject = useStore((s) => s.setProject);
  const pushHistory = useStore((s) => s.pushHistory);
  const setRendered = useStore((s) => s.setRendered);
  const pushActivity = useStore((s) => s.pushActivity);
  const [draft, setDraft] = useState<string | null>(null);
  const [offer, setOffer] = useState<{ from: string; to: string } | null>(null);
  const [state, setState] = useState<"idle" | "busy" | "added">("idle");
  const [error, setError] = useState<string | null>(null);

  async function persist(text: string) {
    setDraft(null);
    if (text === seg.src_text) return;
    if (project) pushHistory(project);
    setRendered(false);
    try {
      setProject(await api.patch(pid, { op: "segment", id: seg.id, src_text: text }));
      setOffer(wordSwap(seg.src_text, text));
      setState("idle");
      setError(null);
    } catch (e) {
      pushActivity(String(e), "error");
      setError(String(e));
    }
  }

  async function addToGlossary() {
    if (!offer) return;
    setState("busy");
    setError(null);
    try {
      const g = await api.glossary(pid);
      const same = (x: string) => x.trim().toLowerCase() === offer.to.toLowerCase();
      const found = g.entries.find((e) => same(e.term));
      const entries = found
        ? g.entries.map((e) => (e === found && !e.asr_fix.some((v) => v.toLowerCase() === offer.from.toLowerCase()) ? { ...e, asr_fix: [...e.asr_fix, offer.from] } : e))
        : [...g.entries, { term: offer.to, translation: "", keep: false, pronunciation: "", asr_fix: [offer.from], note: "", source: "manual" as const, lang: "" }];
      await api.saveGlossary(pid, { entries });
      setState("added");
    } catch (e) {
      setState("idle");
      setError(String(e));
    }
  }

  return (
    <div onClick={(e) => e.stopPropagation()}>
      <textarea value={draft ?? seg.src_text} rows={1} title={t("glossary.srcHint")} aria-label={t("glossary.srcHint")}
        onChange={(e) => setDraft(e.target.value)} onBlur={(e) => { void persist(e.target.value); }}
        className="w-full mt-1.5 bg-transparent border border-transparent rounded-md px-1 -mx-1 text-[11px] leading-snug text-[var(--color-muted)]/80 resize-none overflow-hidden [field-sizing:content] hover:border-[var(--color-border)] focus:border-[var(--color-accent)] focus:text-[var(--color-text)] focus:outline-none transition-colors" />
      {offer && (
        <div className="mt-1 flex flex-wrap items-center gap-1.5 rounded-md border border-[var(--color-accent)]/40 bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)] px-2 py-1 text-[11px]">
          <BookA size={12} className="shrink-0 text-[var(--color-accent)]" />
          <span className="min-w-0 flex-1">
            {state === "added" ? t("glossary.suggestAdded", { term: offer.to }) : t("glossary.suggest", { term: offer.to, from: offer.from })}
          </span>
          {state !== "added" && (
            <button onClick={() => { void addToGlossary(); }} disabled={state === "busy"}
              className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold disabled:opacity-60">
              {state === "busy" ? <Loader2 size={11} className="animate-spin" /> : <Check size={11} />}{t("glossary.suggestAdd")}
            </button>
          )}
          <button onClick={() => setOffer(null)} title={t("glossary.dismiss")} className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={12} /></button>
          {error && <span className="w-full mono text-[10px] text-[var(--color-warn)] break-words">{error}</span>}
        </div>
      )}
      {!offer && error && <div className="mt-1 mono text-[10px] text-[var(--color-warn)] break-words">{error}</div>}
    </div>
  );
}

// Фраза не озвучивается: после чистки текста для синтеза в ней не осталось слов.
export function TtsIgnoredNote({ tags }: { tags: string[] }) {
  const { t } = useTranslation();
  const named = tags.map((tag) => (tag === "+" ? t("glossary.ttsIgnoredStress") : tag)).join(" ");
  return (
    <div className="mt-1 inline-flex items-center gap-1 text-[10.5px] text-[var(--color-warn)]" title={t("glossary.ttsIgnoredHint")}>
      <TagIcon size={11} className="shrink-0" />{t("glossary.ttsIgnored", { tags: named })}
    </div>
  );
}

export function TtsSkipNote({ reason }: { reason: TtsSkip }) {
  const { t } = useTranslation();
  return (
    <div className="mt-1 inline-flex items-center gap-1 text-[10.5px] text-[var(--color-warn)]" title={t("glossary.ttsSkipHint")}>
      <VolumeX size={11} className="shrink-0" />{t(`glossary.ttsSkip.${reason}`)}
    </div>
  );
}
