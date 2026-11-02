import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./tokens.css";
import "./styles.css";
import "./local-models.css";
import "./workspace.css";

const root = document.getElementById("root");
if (!root) throw new Error("ArcMeter root element is missing");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
