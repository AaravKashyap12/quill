// Builds the source app icon: a 1024×1024 ink-navy rounded square with the
// Quill mark centred in cream, then hands it to `tauri icon` to regenerate
// every platform size (Windows .ico, macOS .icns, PNG variants).
//
//   node scripts/build-app-icon.mjs
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const require = createRequire(import.meta.url);
const ROOT = path.resolve(
  path.dirname(new URL(import.meta.url).pathname).replace(/^\/([A-Za-z]:)/, "$1"),
  "..",
);

function loadSharp() {
  const tries = ["sharp", path.join(ROOT, "node_modules/sharp")];
  const pnpmDir = path.join(ROOT, "node_modules/.pnpm");
  if (fs.existsSync(pnpmDir)) {
    for (const entry of fs.readdirSync(pnpmDir)) {
      if (entry.startsWith("sharp@")) tries.push(path.join(pnpmDir, entry, "node_modules/sharp"));
    }
  }
  for (const candidate of tries) {
    try {
      return require(candidate);
    } catch {
      /* next */
    }
  }
  throw new Error("Could not load `sharp`. Run:  npm i -D sharp");
}

const sharp = loadSharp();
const SIZE = 1024;
const RADIUS = 224;
const INK = "#0b1d2a";
const CREAM = "#f2eee6";
const CHAMPAGNE = "#d4b483";

const markSrc = path.join(ROOT, "apps/desktop/src/assets/quill-mark.png");
if (!fs.existsSync(markSrc)) {
  throw new Error(`Missing desktop mark at ${markSrc}`);
}

// Start from the already-cropped, transparent-background mark and recolour
// each visible pixel: keep the gold nib as champagne, flip everything else
// to cream so the mark reads on the ink-navy tile.
const rawMark = await sharp(markSrc)
  .resize({ height: 620 })
  .ensureAlpha()
  .raw()
  .toBuffer({ resolveWithObject: true });

const pixels = rawMark.data;
for (let i = 0; i < pixels.length; i += 4) {
  const r = pixels[i];
  const g = pixels[i + 1];
  const b = pixels[i + 2];
  const a = pixels[i + 3];
  if (a === 0) continue;
  // Detect the gold nib: warm and reasonably saturated.
  const isGold = r > g + 12 && g > b + 6 && r - b > 40;
  const [tr, tg, tb] = isGold ? [0xd4, 0xb4, 0x83] : [0xf2, 0xee, 0xe6];
  pixels[i] = tr;
  pixels[i + 1] = tg;
  pixels[i + 2] = tb;
}

const markLayer = await sharp(pixels, {
  raw: { width: rawMark.info.width, height: rawMark.info.height, channels: 4 },
})
  .png()
  .toBuffer();
const markMeta = await sharp(markLayer).metadata();

// Rounded-square dark background.
const roundedRect = Buffer.from(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${SIZE}" height="${SIZE}">
     <rect x="0" y="0" width="${SIZE}" height="${SIZE}" rx="${RADIUS}" ry="${RADIUS}" fill="${INK}"/>
   </svg>`,
);

// A subtle radial highlight so the tile has depth (~4% opacity champagne).
const glow = Buffer.from(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${SIZE}" height="${SIZE}">
     <defs><radialGradient id="g" cx="30%" cy="15%" r="80%">
       <stop offset="0%" stop-color="${CHAMPAGNE}" stop-opacity="0.10"/>
       <stop offset="55%" stop-color="${CHAMPAGNE}" stop-opacity="0"/>
     </radialGradient></defs>
     <rect x="0" y="0" width="${SIZE}" height="${SIZE}" rx="${RADIUS}" ry="${RADIUS}" fill="url(#g)"/>
   </svg>`,
);

const OUT_DIR = path.join(ROOT, "apps/desktop/src-tauri/icons");
const source = path.join(OUT_DIR, "icon-source.png");
await sharp(roundedRect)
  .composite([
    { input: glow, left: 0, top: 0 },
    {
      input: markLayer,
      left: Math.round((SIZE - markMeta.width) / 2),
      top: Math.round((SIZE - markMeta.height) / 2),
    },
  ])
  .png({ compressionLevel: 9 })
  .toFile(source);
console.log(`wrote ${source}`);

// Hand off to the Tauri CLI to regenerate every platform size (.ico, .icns,
// PNG variants, Store logos). Uses the locally installed tauri-cli.
const tauriBin = path.join(
  ROOT,
  "apps/desktop/node_modules/.bin",
  process.platform === "win32" ? "tauri.CMD" : "tauri",
);
console.log("running tauri icon…");
execSync(`"${tauriBin}" icon "${source}" -o "${OUT_DIR}"`, {
  cwd: path.join(ROOT, "apps/desktop"),
  stdio: "inherit",
});
console.log("done.");
