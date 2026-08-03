import markUrl from "../assets/quill-mark.png";
import wordmarkUrl from "../assets/quill-wordmark.png";

export function BrandMark({ compact = false }: { compact?: boolean }) {
  return (
    <div className="brand-mark" aria-label="Quill">
      <img className="brand-glyph" src={markUrl} alt="" aria-hidden="true" />
      {compact ? null : <img className="brand-name" src={wordmarkUrl} alt="Quill" />}
    </div>
  );
}
