import { useTranslation } from "react-i18next";
import ConfirmDialog from "./ConfirmDialog";
import { useCloudConsent } from "../lib/cloudConsent";

/** The first cloud call of a stage: what leaves this computer and to whom, before it does. */
export default function CloudConsentHost() {
  const { t } = useTranslation();
  const asking = useCloudConsent((s) => s.asking);
  if (!asking) return null;
  return (
    <ConfirmDialog
      title={t("consent.title", { provider: asking.provider })}
      message={`${t(`consent.what_${asking.stage}`, { provider: asking.provider })} ${t("consent.once")}`}
      confirmLabel={t("consent.allow")}
      onConfirm={() => asking.answer(true)}
      onCancel={() => asking.answer(false)}
    />
  );
}
