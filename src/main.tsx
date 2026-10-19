import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

// Disable the WebView context menu & browser drag behaviors for a native feel.
document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
