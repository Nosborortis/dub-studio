import { create } from "zustand";
import { api, llmProviderOf, slot, type Selection } from "./api";
import i18n from "./i18n";

/** The stages that can send the video's sound, text or frames out of this computer (SmartSub, Open Notebook:
 * ask before the first time). */
export type CloudStage = "recognition" | "translation" | "vision" | "voicing";
export const CLOUD_STAGES: readonly CloudStage[] = ["recognition", "translation", "vision", "voicing"];
export const consentKey = (stage: CloudStage) => `cloud_ok_${stage}`;

/** The cloud a stage goes to now, by name as the person knows it; null while it runs on this computer. */
export function cloudOf(sel: Selection | undefined, stage: CloudStage): string | null {
  switch (stage) {
    case "recognition":
      return slot(sel, "or_asr_on") === "1" && (slot(sel, "or_asr") ?? "").trim() ? "OpenRouter" : null;
    case "translation":
      return llmProviderOf(sel, "llm") === "openrouter" ? "OpenRouter" : null;
    case "vision":
      return llmProviderOf(sel, "vision") === "openrouter" ? "OpenRouter" : null;
    case "voicing": {
      const provider = slot(sel, "tts_provider") ?? (slot(sel, "or_tts_on") === "1" ? "openrouter" : "local");
      return provider === "google" ? "Gemini" : provider === "openrouter" ? "OpenRouter" : null;
    }
  }
}

type Asking = { stage: CloudStage; provider: string; answer: (ok: boolean) => void };

/** The question the window shows now, answered by CloudConsentHost. */
export const useCloudConsent = create<{ asking: Asking | null }>(() => ({ asking: null }));

/** Whether these stages may go to the cloud: each one that would and was not allowed yet is asked once, and the
 * answer is kept in the studio's settings. False as soon as the person keeps one on this computer. */
export async function askCloud(stages: readonly CloudStage[]): Promise<boolean> {
  const sel = (await api.capabilities()).selection;
  for (const stage of stages) {
    const provider = cloudOf(sel, stage);
    if (!provider || slot(sel, consentKey(stage)) === "1") continue;
    const ok = await new Promise<boolean>((answer) => useCloudConsent.setState({ asking: { stage, provider, answer } }));
    useCloudConsent.setState({ asking: null });
    if (!ok) return false;
    await api.setSelection(consentKey(stage), "1");
  }
  return true;
}

/** The same question before a call of the window that starts these stages: a stage the person keeps on this computer
 * stops the call with a reason. */
export async function requireCloud(stages: readonly CloudStage[]): Promise<void> {
  if (!(await askCloud(stages))) throw new ConsentDeclined(i18n.t("consent.declined"));
}

/** The person's own choice, not a failure: read as its words wherever the window shows an error, `String(e)` too. */
export class ConsentDeclined extends Error {
  override toString(): string {
    return this.message;
  }
}
