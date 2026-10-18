import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import TrayDropPanel from "./panels/TrayDropPanel";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/app.css";
import "./styles/ios.css";

const root = document.getElementById("root");
if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      {new URLSearchParams(window.location.search).get("panel") === "tray-drop" ? <TrayDropPanel /> : <App />}
    </React.StrictMode>,
  );
}
