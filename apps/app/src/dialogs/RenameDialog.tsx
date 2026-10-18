import { useEffect, useState } from "react";
import { Button, Field, Modal, TextInput } from "../components/ui";
import { t } from "../i18n";
import { renameDevice } from "../store";
import type { DeviceView } from "../types";

/** Sets or clears the local name of a device; open while `device` is set. */
export function RenameDialog({ device, onClose }: { device: DeviceView | null; onClose: () => void }) {
  const [alias, setAlias] = useState("");
  useEffect(() => setAlias(device?.customAlias ?? ""), [device]);
  return (
    <Modal
      open={device !== null}
      title={t("devices.renameTitle")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("app.cancel")}
          </Button>
          <Button
            onClick={() => {
              if (device) void renameDevice(device, alias.trim() || null);
              onClose();
            }}
          >
            {t("app.save")}
          </Button>
        </>
      }
    >
      <Field label={device?.alias ?? ""} hint={t("devices.renameHint")}>
        <TextInput autoFocus value={alias} onChange={(e) => setAlias(e.target.value)} placeholder={device?.alias} />
      </Field>
    </Modal>
  );
}
