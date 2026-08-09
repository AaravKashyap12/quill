import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { VoiceMotionLab } from "./components/VoiceMotionLab";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("Quill root element is missing");

const windowQuery = new URLSearchParams(window.location.search);
const motionLab = windowQuery.has("motionLab");
if (windowQuery.has("demo")) document.documentElement.dataset.demo = "true";
const surface = motionLab
  ? "motion-lab"
  : windowQuery.has("overlay")
  ? "overlay"
  : windowQuery.has("review")
    ? "review"
    : "settings";
document.documentElement.dataset.surface = surface;
document.title = surface === "review" ? "Review Scribe draft" : "Quill";

createRoot(root).render(
  <StrictMode>
    {motionLab ? <VoiceMotionLab /> : <App />}
  </StrictMode>,
);
