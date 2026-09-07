import { useState } from "react";
import { Button, Modal, TextInput } from "../components/ui";
import { t } from "../i18n";
import { providePin, useStore } from "../store";

export function PinDialog() {
  const request = useStore((s) => s.pinRequests[0]);
  const [pin, setPin] = useState("");
  if (!request) return null;
  const submit = () => {
    if (pin.trim()) {
      void providePin(request.transferId, pin.trim());
      setPin("");
    }
  };
  return (
    <Modal
      open
      title={t("pin.title")}
      onClose={() => void providePin(request.transferId, null)}
      footer={
        <>
          <Button variant="ghost" onClick={() => void providePin(request.transferId, null)}>
            {t("app.cancel")}
          </Button>
          <Button disabled={!pin.trim()} onClick={submit}>
            {t("pin.submit")}
          </Button>
        </>
      }
    >
      <p className="muted" style={{ marginTop: 0 }}>
        {t("pin.hint")}
      </p>
      <TextInput
        className="code"
        autoFocus
        inputMode="numeric"
        placeholder={t("pin.placeholder")}
        value={pin}
        onChange={(e) => setPin(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && submit()}
      />
    </Modal>
  );
}
