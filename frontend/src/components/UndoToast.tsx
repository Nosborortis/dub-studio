import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Undo2, X } from "lucide-react";
import { named } from "../lib/a11y";

const SHOW_MS = 10_000;

/** What was just removed, with a way to take it back while the note is shown. */
export default function UndoToast({ text, onUndo, onClose }: { text: string; onUndo: () => Promise<void>; onClose: () => void }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (busy || error) return;
    const id = window.setTimeout(onClose, SHOW_MS);
    return () => window.clearTimeout(id);
  }, [busy, error, onClose]);
  const undo = async () => {
    setBusy(true);
    setError(null);
    try { await onUndo(); onClose(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };
  return (
    <div role="status" className="fixed bottom-24 left-1/2 -translate-x-1/2 z-[85] max-w-[min(calc(100vw-2rem),520px)] flex items-center gap-3 px-4 py-2.5 rounded-xl border border-[var(--color-border)] bg-[var(--color-overlay)] shadow-xl text-[13px] anim-pop">
      <span className="min-w-0 flex-1">
        <span className="block truncate">{text}</span>
        {error && <span role="alert" className="block text-[12px] text-[var(--color-warn)] break-words">{error}</span>}
      </span>
      <button type="button" onClick={() => void undo()} disabled={busy} className="shrink-0 inline-flex items-center gap-1.5 px-3 py-1 rounded-lg border border-[var(--color-accent)] text-[var(--color-accent)] text-[12px] font-semibold disabled:opacity-40">
        {busy ? <Loader2 size={13} className="animate-spin" /> : <Undo2 size={13} />}{t("undoDelete.restore")}
      </button>
      <button type="button" onClick={onClose} className="shrink-0 p-1 rounded text-[var(--color-muted)] hover:text-[var(--color-text)]" {...named(t("prefs.close"))}><X size={14} /></button>
    </div>
  );
}
