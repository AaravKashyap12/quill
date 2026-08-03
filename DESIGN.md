# Quill Design System

## Direction

Quill feels like a precision writing instrument: an inked quill that resolves into a live soundwave. The surface is calm and literary until speech moves through it, at which point the champagne signal and the waveform come alive. The desktop product stays restrained and quiet; the marketing site is where the identity is expressed — warm paper, deep ink, and one artistic dark section per scroll.

- `DESIGN_VARIANCE: 7`
- `MOTION_INTENSITY: 5`
- `VISUAL_DENSITY: 4`
- Physical reference: a fountain-pen manuscript on warm paper, annotated in emerald, with a champagne-gold proofing mark.
- Dictation is mechanical and immediate. Scribe settles a spoken correction into clean, resolved text.

## Color

Identity palette (from the Quill brand board). Exact brand hexes are preserved; tints are derived with `color-mix`.

```css
:root {
  --ink: #0b1d2a;        /* Ink Navy — primary text + dark surfaces */
  --emerald: #0f4c46;    /* Emerald — brand primary, CTAs */
  --emerald-bright: #17756a;
  --champagne: #d4b483;  /* Champagne — live / signal accent */
  --gold-deep: #7a5a24;  /* accessible gold for small text on light */
  --sand: #f2eee6;       /* Sand — warm raised surface */
  --mist: #8c96a1;
  --paper: #f8f7f2;      /* body background */
  --paper-raised: #fffefb;
  --muted: #4d5964;      /* muted text — ≥4.5:1 on paper/sand */
}
```

- **Ink Navy** carries all body text on light and is the drench color for dark sections (Privacy, the overlay showcase, Closing).
- **Emerald** is the single brand primary: download button, links, mode accents, live caret. It carries ~30% of the surface via the dark sections.
- **Champagne** is the live/signal accent only — the recording waveform, the "no cloud" boundary, the Scribe correction highlight. Never a body color.
- Color is never the only signal: every mode carries an icon + text label as well.

## Typography

- Display: **Spectral** (`--font-display`, via `next/font`) — a literary high-contrast serif. Its italic carries emphasis (`Speak.`, `every`, `meant`, `Your machine.`).
- Body / UI: **Onest Variable** (`--font-body`) — calm humanist sans.
- Micro-labels / kickers: **Schibsted Grotesk Variable** (`--font-label`) — used sparingly for structure, not as an eyebrow on every section.
- Transcripts / hotkeys / code: native mono stack (`--font-mono`).
- Hero display: `clamp(3.2rem, 6.6vw, 5.4rem)` at 0.98 line height. Section titles: `clamp(2rem, 3.8vw, 3.7rem)`. Letter-spacing on display no tighter than -0.032em. Body caps at ~65ch.

## Shape and Depth

- Controls & buttons: fully rounded (999px) — they read as transient, instrument-like.
- Panels & cards: 20px radius (`--r-lg`); inner elements 12px (`--r-md`).
- Recording overlay: fully rounded pill.
- Depth comes from soft, low, tinted shadows (`… -40px color-mix(ink)`) plus 1px inset highlights, not hard registration shadows.

## Layout

- Marketing: max-width 1280px, asymmetric section rhythm, one dark drenched section per few light ones. No repeated identical feature-card grid; the two-mode comparison is the one intentional 2-up.
- Fluid spacing via `clamp()`; sections breathe at `clamp(72px, 9vw, 132px)` vertical.

### Desktop app

- A 224px navigation rail on warm `--surface` + a flexible content pane, collapsing to a horizontal tab rail below 680px.
- The rail carries the real logo lockup. The active row is marked three ways — an emerald hairline on the leading edge, a raised white pill, and an emerald icon — so it never depends on colour alone.
- Content is capped at 780px and led by a serif `h1`; section labels are small uppercase grotesque, which keeps the hierarchy legible without competing with the headline.
- Settings are grouped into one bordered sheet per section rather than a run of loose rows: rows are separated by hairlines inside a single rounded card, each with a hover wash.
- The app never fetches type over the network. Headings resolve to Spectral if present, otherwise Cambria / Iowan Old Style / Georgia; UI stays on the platform sans so controls feel native.

## Motion

- Hero: the ink flourish draws its quill stroke, then the champagne soundwave bars pop in. The compose window rises.
- The waveform is the primary living element — it scales amplitude in place, never moving layout.
- Scribe's resolved word settles in (`settle`), the live dot pulses (`live-pulse`), the caret blinks.
- Product state transitions 180–260ms, ease-out. Everything degrades to instant / crossfade under `prefers-reduced-motion`.

## Components

- `RecordingOverlay`: compact bottom-centre pill — mode badge, live waveform, mode name + hotkey/lock hint. Small and gone on key release.
- `ModeShortcut` / `SettingRow`: unchanged desktop primitives.
- `DownloadPicker`: platform-aware split button, emerald.
- `Comparison`: one honest table vs Wispr Flow and Willow Voice — the local-first trade-off, not a scorecard.

## Voice

Short, specific, non-anthropomorphic. Quill "transcribes," "holds," "resolves," and "types." It does not "think," "understand you," or promise magical writing improvement. Cut filler ("Made with care. For everyone. Forever." and similar).
