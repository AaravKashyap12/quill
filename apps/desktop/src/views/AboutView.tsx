import { ExternalLink, Github, HardDrive, History, ShieldCheck } from "lucide-react";
import desktopPackage from "../../package.json";
import markUrl from "../assets/quill-mark.png";

export function AboutView() {
  return (
    <div className="view-stack about-view">
      <header className="view-heading">
        <h1>About Quill</h1>
        <p>A local writing instrument built around two global shortcuts.</p>
      </header>

      <section className="about-intro">
        <img className="about-mark" src={markUrl} alt="" aria-hidden="true" />
        <div>
          <h2>Quill {desktopPackage.version}</h2>
          <p>
            Dictation types what you say. Scribe refines it for your approval.
            Speech and writing models run locally, without an account.
          </p>
        </div>
      </section>

      <div className="about-facts">
        <div>
          <HardDrive size={18} aria-hidden="true" />
          <span><strong>Local speech</strong>whisper.cpp runs with CPU, CUDA, or Metal</span>
        </div>
        <div>
          <ShieldCheck size={18} aria-hidden="true" />
          <span><strong>No telemetry</strong>Quill has no analytics client</span>
        </div>
      </div>

      <section className="about-project">
        <div className="section-heading">
          <h2>Project</h2>
        </div>
        <div className="link-list">
          <a href="https://github.com/AaravKashyap12/quill" target="_blank" rel="noreferrer">
            <Github size={17} aria-hidden="true" />
            <span>Source code</span>
            <ExternalLink size={14} aria-hidden="true" />
          </a>
          <a href="https://github.com/AaravKashyap12/quill/releases" target="_blank" rel="noreferrer">
            <History size={17} aria-hidden="true" />
            <span>Release notes</span>
            <ExternalLink size={14} aria-hidden="true" />
          </a>
        </div>
      </section>
    </div>
  );
}
