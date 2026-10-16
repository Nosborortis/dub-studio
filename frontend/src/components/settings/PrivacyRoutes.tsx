import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Cloud, HardDrive } from "lucide-react";
import { api, slot, type Selection } from "../../lib/api";
import { CLOUD_STAGES, cloudOf, consentKey, type CloudStage } from "../../lib/cloudConsent";

/** Where each stage's sound, text and frames go now: this computer or a cloud provider; and the stages allowed to go
 * to the cloud, with a way to be asked again. */
export default function PrivacyRoutes() {
  const { t } = useTranslation();
  const [sel, setSel] = useState<Selection | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => { api.capabilities().then((c) => setSel(c.selection ?? {}), (e: unknown) => setProblem(e instanceof Error ? e.message : String(e))); }, []);
  const askAgain = (stage: CloudStage) => {
    api.setSelection(consentKey(stage), "0").then(setSel, (e: unknown) => setProblem(e instanceof Error ? e.message : String(e)));
  };
  return (
    <section className="space-y-2">
      <h4 className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)]">{t("privacyRoutes.title")}</h4>
      <p className="text-[12px] text-[var(--color-muted)] leading-relaxed">{t("privacyRoutes.hint")}</p>
      <div className="rounded-lg border border-[var(--color-border)] divide-y divide-[var(--color-border)] bg-[var(--color-surface-2)]/40">
        {CLOUD_STAGES.map((stage) => {
          const provider = sel ? cloudOf(sel, stage) : null;
          const allowed = slot(sel ?? undefined, consentKey(stage)) === "1";
          return (
            <div key={stage} className="flex items-start gap-3 px-3 py-2">
              <span className="mt-0.5 shrink-0">{provider ? <Cloud size={13} className="text-[var(--color-warn)]" /> : <HardDrive size={13} className="text-[var(--color-accent)]" />}</span>
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium">{t(`privacyRoutes.stage_${stage}`)}</div>
                <div className="text-[11px] leading-snug text-[var(--color-muted)]">{provider ? t("privacyRoutes.cloud", { provider }) : t("privacyRoutes.local")}</div>
              </div>
              {provider && (allowed
                ? <button type="button" onClick={() => askAgain(stage)} className="shrink-0 text-[11px] underline text-[var(--color-muted)] hover:text-[var(--color-text)]">{t("privacyRoutes.askAgain")}</button>
                : <span className="shrink-0 text-[11px] text-[var(--color-muted)]">{t("privacyRoutes.willAsk")}</span>)}
            </div>
          );
        })}
      </div>
      {problem && <p role="alert" className="text-[11px] text-[var(--color-warn)] break-words">{problem}</p>}
    </section>
  );
}
