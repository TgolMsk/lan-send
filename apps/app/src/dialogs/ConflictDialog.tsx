import { Button, Modal } from "../components/ui";
import { baseName } from "../format";
import { t } from "../i18n";
import { respondConflict, useStore } from "../store";

export function ConflictDialog() {
  const conflict = useStore((s) => s.conflicts[0]);
  if (!conflict) return null;
  return (
    <Modal
      open
      title={t("conflict.title")}
      footer={
        <>
          <Button variant="outline" onClick={() => void respondConflict(conflict, true)}>
            {t("conflict.overwrite")}
          </Button>
          <Button autoFocus onClick={() => void respondConflict(conflict, false)}>
            {t("conflict.rename")} · {baseName(conflict.renamed)}
          </Button>
        </>
      }
    >
      <p style={{ margin: 0 }}>{t("conflict.hint", { name: baseName(conflict.existing) })}</p>
    </Modal>
  );
}
