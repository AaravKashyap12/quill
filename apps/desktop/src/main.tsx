import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("Quill root element is missing");

const windowQuery = new URLSearchParams(window.location.search);
const surface = windowQuery.has("overlay")
  ? "overlay"
  : windowQuery.has("review")
    ? "review"
    : "settings";
document.documentElement.dataset.surface = surface;
document.title = surface === "review" ? "Review Scribe draft" : "Quill";

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
