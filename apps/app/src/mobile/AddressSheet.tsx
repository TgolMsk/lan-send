// "Send by address" on iOS: type an IP, then pick what to send or pair.

import { useState } from "react";
import { Icon } from "../components/icons";
import { Button, Modal, TextInput } from "../components/ui";
import { t } from "../i18n";
import { pickFiles, sendFiles, startPair } from "../store";
import type { PickKind } from "../types";

export function AddressSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [address, setAddress] = useState("");
  const target = address.trim();
  const send = async (kind: PickKind) => {
    const paths = await pickFiles(kind);
    if (!paths.length) return;
    onClose();
    await sendFiles(target, paths);
  };
  return (
    <Modal
      open={open}
      title={t("devices.direct")}
      onClose={onClose}
      footer={
        <Button
          variant="outline"
          icon="link"
          disabled={!target}
          onClick={() => {
            onClose();
            void startPair(target);
          }}
        >
          {t("devices.pair")}
        </Button>
      }
    >
      <TextInput
        autoFocus
        inputMode="url"
        autoCapitalize="off"
        autoCorrect="off"
        placeholder={t("devices.directPlaceholder")}
        value={address}
        onChange={(e) => setAddress(e.target.value)}
      />
      <div className="m-send" style={{ marginTop: 14 }}>
        <button type="button" className="m-send-btn primary compact" disabled={!target} onClick={() => void send("media")}>
          <Icon name="image" size={24} />
          {t("devices.sendMedia")}
        </button>
        <button type="button" className="m-send-btn compact" disabled={!target} onClick={() => void send("files")}>
          <Icon name="file" size={24} />
          {t("devices.sendFiles")}
        </button>
      </div>
    </Modal>
  );
}
