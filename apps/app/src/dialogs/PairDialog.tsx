import { Button, Modal, Spinner } from "../components/ui";
import { t } from "../i18n";
import { closePair, confirmPair, useStore } from "../store";

export function PairDialog() {
  const flow = useStore((s) => s.pair);
  if (!flow) return null;
  const outgoing = flow.kind === "outgoing";

  let body;
  let footer;
  if (flow.stage === "done") {
    body = (
      <p style={{ margin: 0 }}>
        {flow.paired ? t("pair.success", { alias: flow.alias }) : `${t("pair.failed")}${flow.message ? ` · ${flow.message}` : ""}`}
      </p>
    );
    footer = <Button onClick={closePair}>{t("app.close")}</Button>;
  } else if (flow.stage === "waiting") {
    body = (
      <>
        <div className="code-display">{flow.code || "······"}</div>
        <p className="muted" style={{ margin: 0, textAlign: "center" }}>
          {t("pair.codeHint")}
        </p>
        <div className="row" style={{ justifyContent: "center", marginTop: 16 }}>
          <Spinner /> <span className="muted">{t("pair.waiting", { alias: flow.alias })}</span>
        </div>
      </>
    );
    footer = (
      <Button variant="ghost" onClick={closePair}>
        {t("app.cancel")}
      </Button>
    );
  } else {
    body = (
      <>
        {!outgoing && <p style={{ marginTop: 0 }}>{t("pair.request", { alias: flow.alias })}</p>}
        <div className="code-display">{flow.code}</div>
        <p className="muted" style={{ margin: 0, textAlign: "center" }}>
          {outgoing ? t("pair.confirmQuestion", { alias: flow.alias }) : t("pair.codeHint")}
        </p>
      </>
    );
    footer = (
      <>
        <Button variant="outline" onClick={() => void confirmPair(false)}>
          {outgoing ? t("pair.mismatch") : t("pair.reject")}
        </Button>
        <Button icon="link" autoFocus onClick={() => void confirmPair(true)}>
          {outgoing ? t("pair.matches") : t("pair.accept")}
        </Button>
      </>
    );
  }
  return (
    <Modal open title={t("pair.title")} onClose={flow.stage === "done" ? closePair : undefined} footer={footer}>
      {body}
    </Modal>
  );
}
