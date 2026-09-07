import { Button, Modal } from "../components/ui";
import { t } from "../i18n";

export function ConfirmDialog({
  open,
  title,
  text,
  danger = false,
  onConfirm,
  onClose,
}: {
  open: boolean;
  title: string;
  text: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <Modal
      open={open}
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("app.cancel")}
          </Button>
          <Button variant={danger ? "danger" : "primary"} onClick={onConfirm}>
            {t("app.confirm")}
          </Button>
        </>
      }
    >
      <p style={{ margin: 0 }}>{text}</p>
    </Modal>
  );
}
