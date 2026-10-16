import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Bug, Check, Copy, FolderOpen, Loader2, Package, RefreshCw } from "lucide-react";
import { api, type GpuReport } from "../../lib/api";
import { issueReportUrl } from "../../lib/links";

type Diagnostics = { text: string; report: { gpu: GpuReport } };

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** The system as the studio sees it (card, driver, CUDA, ffmpeg, components, missing DLLs, the latest log), to copy into
 * an issue or to pack for a bug report; keys never get in. */
export default function DiagnosticsPanel() {
  const { t } = useTranslation();
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [packing, setPacking] = useState(false);
  const [pack, setPack] = useState<string | null>(null);
  const load = useCallback(() => {
    api.diagnostics().then((d) => { setDiag(d); setProblem(null); }, (e: unknown) => setProblem(errorText(e)));
  }, []);
  useEffect(load, [load]);
  const copy = () => {
    if (!diag) return;
    navigator.clipboard.writeText(diag.text).then(() => { setCopied(true); window.setTimeout(() => setCopied(false), 1500); }, (e: unknown) => setProblem(errorText(e)));
  };
  const makePack = async () => {
    setPacking(true);
    try { setPack((await api.diagnosticsReport()).path); setProblem(null); } catch (e) { setProblem(errorText(e)); } finally { setPacking(false); }
  };
  const reveal = () => { if (pack) api.revealReport(pack).catch((e: unknown) => setProblem(errorText(e))); };
  const btn = "inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[12px] hover:border-[var(--color-accent)] disabled:opacity-40";
  return (
    <section className="mb-5" data-settings-part="system">
      <h4 className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)] mb-2">{t("diag.title")}</h4>
      <p className="mb-2 text-[12px] text-[var(--color-muted)] leading-relaxed">{t("diag.hint")}</p>
      <div className="flex flex-wrap gap-2 mb-2">
        <button type="button" onClick={copy} disabled={!diag} className={btn}>{copied ? <Check size={13} className="text-[var(--color-accent)]" /> : <Copy size={13} />}{copied ? t("prefs.copied") : t("diag.copy")}</button>
        <button type="button" onClick={() => void makePack()} disabled={packing} className={btn}>{packing ? <Loader2 size={13} className="animate-spin" /> : <Package size={13} />}{t("diag.pack")}</button>
        <a href={issueReportUrl(__APP_VERSION__, diag?.report.gpu)} target="_blank" rel="noreferrer" className={btn}><Bug size={13} />{t("diag.report")}</a>
        <button type="button" onClick={() => api.revealLogs().catch((e: unknown) => setProblem(errorText(e)))} className={btn}><FolderOpen size={13} />{t("diag.logs")}</button>
        <button type="button" onClick={load} className={btn}><RefreshCw size={13} />{t("diag.refresh")}</button>
      </div>
      {pack && (
        <p className="mb-2 flex flex-wrap items-center gap-2 text-[11px]">
          <span className="text-[var(--color-muted)]">{t("diag.packed")}</span>
          <span className="mono break-all">{pack}</span>
          <button type="button" onClick={reveal} className="inline-flex items-center gap-1 underline text-[var(--color-muted)] hover:text-[var(--color-text)]"><FolderOpen size={12} />{t("diag.show")}</button>
        </p>
      )}
      {problem && <p role="alert" className="mb-2 mono text-[11px] text-[var(--color-warn)] break-words">{problem}</p>}
      {diag ? <pre className="max-h-72 overflow-auto rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] p-2 mono text-[11px] leading-snug whitespace-pre-wrap break-words">{diag.text}</pre>
        : !problem && <Loader2 size={14} className="animate-spin text-[var(--color-muted)]" />}
    </section>
  );
}
