import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { invoke } from "@tauri-apps/api/core";
import "./styles/index.css";
import "./styles/shared.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

// Сторож в ядре ждёт этот сигнал; без него через несколько секунд покажет
// объяснение, почему окно пустое (прокси, антивирус, WebView2).
invoke("panel_ready").catch(() => {});
