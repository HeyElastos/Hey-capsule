import React from "react";
import { createRoot } from "react-dom/client";
import App from "./App.jsx";
import { initSession } from "./lib/session.js";
import "./index.css";

// Populate the in-memory keypair cache (from IDB) before React mounts —
// every signed-event helper assumes session.getKeypair() is non-null for
// signed-in users. Without this, the first render races IDB and signed
// publishes silently fail.
const boot = async () => {
  try { await initSession(); }
  catch (err) { console.warn("[hey-messenger] initSession failed", err); }
  createRoot(document.getElementById("root")).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
};
boot();
