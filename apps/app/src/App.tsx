import { useEffect } from "react";
import { Layout, RuntimeBanner } from "./components/Layout";
import { Toasts } from "./components/Toasts";
import { ConflictDialog } from "./dialogs/ConflictDialog";
import { IncomingDialog } from "./dialogs/IncomingDialog";
import { PairDialog } from "./dialogs/PairDialog";
import { PinDialog } from "./dialogs/PinDialog";
import { SendDialog } from "./dialogs/SendDialog";
import { ClipboardPage } from "./pages/Clipboard";
import { DevicesPage } from "./pages/Devices";
import { HistoryPage } from "./pages/History";
import { SettingsPage } from "./pages/Settings";
import { TransfersPage } from "./pages/Transfers";
import { bootstrap, navigate, useStore, type Page } from "./store";

export default function App() {
  const page = useStore((s) => s.page);
  const mobile = useStore((s) => s.platform.mobile);
  // Re-render on locale changes.
  useStore((s) => s.locale);
  useEffect(() => {
    void bootstrap();
    const wanted = new URLSearchParams(window.location.search).get("page");
    if (wanted && ["devices", "transfers", "clipboard", "history", "settings"].includes(wanted)) navigate(wanted as Page);
  }, []);
  return (
    <>
      <Layout>
        {!mobile && <RuntimeBanner />}
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
