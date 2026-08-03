import { ExternalLink, Github, HardDrive, ShieldCheck } from "lucide-react";
import markUrl from "../assets/quill-mark.png";

export function AboutView() {
  return (
    <div className="view-stack">
      <header className="view-heading">
        <h1>About Quill</h1>
      </header>
      <section className="about-intro">
        <img className="about-mark" src={markUrl} alt="" aria-hidden="true" />
        <div>
          <h2>Quill 0.1.0</h2>
          <p>
            Two hotkeys, one local speech engine, and no account. Quill is an
            independent open-source project released under the GNU AGPL-3.0.
          </p>
        </div>
      </section>
      <div className="about-facts">
        <div>
          <HardDrive size={18} />
          <span><strong>Local speech</strong>whisper.cpp with CPU, CUDA, or Metal</span>
        </div>
        <div>
          <ShieldCheck size={18} />
          <span><strong>No telemetry</strong>Usage stats never leave this device</span>
        </div>
      </div>
      <section>
        <div className="section-heading">
          <h2>Project</h2>
        </div>
        <div className="link-list">
          <a href="https://github.com/AaravKashyap12/quill" target="_blank" rel="noreferrer">
            <Github size={17} />
            Source code
            <ExternalLink size={14} />
          </a>
          <a href="https://github.com/AaravKashyap12/quill/releases" target="_blank" rel="noreferrer">
            Release notes
            <ExternalLink size={14} />
          </a>
        </div>
      </section>
    </div>
  );
}
