import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Bug, Check, CircleAlert, Copy, Stethoscope, X } from "lucide-react";
import { openSettings } from "../lib/settingsNav";
import { issueReportUrl } from "../lib/links";
import { named } from "../lib/a11y";

/** An error that stays until it is closed: what failed and why (the studio's own words), what to do (a hint where
 * the cause is known, else the diagnostics and a report), and the whole text to copy into a report. */
export default function ErrorNote({ text, hint, onClose }: { text: string; hint?: ReactNode; onClose?: () => void }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const copy = () => {
    navigator.clipboard.writeText(text).then(() => { setCopied(true); window.setTimeout(() => setCopied(false), 1500); }, (e: unknown) => console.error("[ERROR] copy failed:", e));
  };
  const btn = "inline-flex items-center gap-1 px-2 py-0.5 rounded border border-[var(--color-warn)]/40 text-[11px] hover:bg-[var(--color-warn)]/10";
  return (
    <div role="alert" className="px-3 py-2 rounded-lg border border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_6%,transparent)] text-[12px]">
      <div className="flex items-start gap-2">
        <CircleAlert size={14} className="mt-0.5 shrink-0 text-[var(--color-warn)]" />
        <p className="min-w-0 flex-1 break-words whitespace-pre-wrap text-[var(--color-text)]">{text}</p>
        {onClose && <button onClick={onClose} {...named(t("a11y.close"))} className="shrink-0 p-0.5 rounded text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={13} /></button>}
      </div>
      <div className="mt-1.5 ml-6 flex flex-wrap items-center gap-1.5 text-[var(--color-muted)]">
        {hint ? <span className="mr-1">{hint}</span> : <span className="mr-1">{t("errors.whatToDo")}</span>}
        <button onClick={copy} className={btn}>{copied ? <Check size={11} /> : <Copy size={11} />}{copied ? t("prefs.copied") : t("errors.copy")}</button>
        <button onClick={() => openSettings("quality:system")} className={btn}><Stethoscope size={11} />{t("errors.diagnostics")}</button>
        <a href={issueReportUrl(__APP_VERSION__)} target="_blank" rel="noreferrer" className={btn}><Bug size={11} />{t("diag.report")}</a>
      </div>
    </div>
  );
}
