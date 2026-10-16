import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, TriangleAlert } from "lucide-react";
import { useEscapeLayer } from "../lib/escape";

// Подтверждение необратимого действия вместо window.confirm: своё окно в стиле студии, Escape и клик по
// подложке отменяют, Enter подтверждает. onConfirm может быть асинхронным: пока он идёт, окно ждёт, а его
// ошибка показывается здесь же, и окно остаётся открытым.
export default function ConfirmDialog({ title, message, confirmLabel, danger = false, disabled = false, onConfirm, onCancel }: {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  /** The action cannot go ahead now (the message says why): only Cancel. */
  disabled?: boolean;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  const messageId = useId();

  const runConfirm = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };

  useEffect(() => { confirmRef.current?.focus(); }, []);
  useEscapeLayer(() => { if (!busy) onCancel(); });

  return (
    <div className="fixed inset-0 z-[70] grid place-items-center glass-scrim anim-fade" onClick={() => { if (!busy) onCancel(); }}>
      <div role="alertdialog" aria-modal="true" aria-labelledby={titleId} aria-describedby={messageId}
        className="w-[min(92vw,420px)] rounded-xl glass-panel anim-pop p-5" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-start gap-3">
          {danger && <TriangleAlert size={18} className="shrink-0 mt-0.5 text-[var(--color-warn)]" />}
          <div className="min-w-0 flex-1">
            <h2 id={titleId} className="font-semibold text-[15px]">{title}</h2>
            <p id={messageId} className="mt-1.5 text-[13px] leading-relaxed text-[var(--color-muted)] break-words">{message}</p>
          </div>
        </div>
        {error && <p role="alert" className="mt-3 mono text-[11px] text-[var(--color-warn)] break-words">{error}</p>}
        <div className="mt-5 flex justify-end gap-2">
          <button type="button" onClick={onCancel} disabled={busy}
            className="px-3.5 py-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[13px] text-[var(--color-text)] hover:border-[#3a414c] disabled:opacity-40 transition-colors">
            {t("common.cancel")}
          </button>
          <button ref={confirmRef} type="button" onClick={() => { void runConfirm(); }} disabled={busy || disabled}
            className={`inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-[13px] font-semibold disabled:opacity-60 transition ${danger ? "bg-[#ef4444] text-white hover:brightness-110" : "bg-[var(--color-accent)] text-[var(--color-on-accent)] hover:brightness-105"}`}>
            {busy && <Loader2 size={13} className="animate-spin" />}{confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
