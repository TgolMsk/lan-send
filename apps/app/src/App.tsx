import { useEffect } from "react";
import { Layout } from "./components/Layout";
import { Toasts } from "./components/Toasts";
import { Button } from "./components/ui";
import { ConflictDialog } from "./dialogs/ConflictDialog";
import { IncomingDialog } from "./dialogs/IncomingDialog";
import { PairDialog } from "./dialogs/PairDialog";
import { PinDialog } from "./dialogs/PinDialog";
import { SendDialog } from "./dialogs/SendDialog";
import { t } from "./i18n";
import { ClipboardPage } from "./pages/Clipboard";
import { DevicesPage } from "./pages/Devices";
import { HistoryPage } from "./pages/History";
import { SettingsPage } from "./pages/Settings";
import { TransfersPage } from "./pages/Transfers";
import { bootstrap, retryRuntime, useStore } from "./store";

export default function App() {
  const page = useStore((s) => s.page);
  const runtime = useStore((s) => s.runtime);
  // Re-render on locale changes.
  useStore((s) => s.locale);
  useEffect(() => {
    void bootstrap();
  }, []);
  return (
    <>
      <Layout>
        {!runtime.running && (
          <div className="banner">
            <span>
              {t("app.runtimeStopped")}
              {runtime.message ? ` · ${runtime.message}` : ""}
            </span>
            <span className="spacer" />
            <Button size="sm" variant="outline" icon="refresh" onClick={() => void retryRuntime()}>
              {t("app.retry")}
            </Button>
          </div>
        )}
        {page === "devices" && <DevicesPage />}
        {page === "transfers" && <TransfersPage />}
        {page === "clipboard" && <ClipboardPage />}
        {page === "history" && <HistoryPage />}
        {page === "settings" && <SettingsPage />}
      </Layout>
      <IncomingDialog />
      <PinDialog />
      <ConflictDialog />
      <PairDialog />
      <SendDialog />
      <Toasts />
    </>
  );
}
