import fs from "node:fs";
import path from "node:path";

const next = process.argv[2];
const checkOnly = process.argv.includes("--check");
const root = path.resolve(process.env.QUILL_ROOT ?? process.cwd());

if (!/^\d+\.\d+\.\d+$/.test(next ?? "")) {
  throw new Error(
    `Version must use MAJOR.MINOR.PATCH without a v prefix; received ${next ?? "nothing"}`,
  );
}

const fromRoot = (relativePath) => path.join(root, relativePath);
const jsonPaths = [
  "package.json",
  "apps/desktop/package.json",
  "apps/desktop/src-tauri/tauri.conf.json",
];

const rootPackage = JSON.parse(fs.readFileSync(fromRoot("package.json"), "utf8"));
const current = rootPackage.version;

if (!checkOnly) {
  const currentParts = current.split(".").map(Number);
  const nextParts = next.split(".").map(Number);
  const isNewer = nextParts.some(
    (part, index) =>
      part > currentParts[index] &&
      nextParts.slice(0, index).every((value, prior) => value === currentParts[prior]),
  );
  if (!isNewer) {
    throw new Error(`New version ${next} must be greater than current version ${current}`);
  }

  for (const relativePath of jsonPaths) {
    const filePath = fromRoot(relativePath);
    const document = JSON.parse(fs.readFileSync(filePath, "utf8"));
    document.version = next;
    fs.writeFileSync(filePath, `${JSON.stringify(document, null, 2)}\n`);
  }

  const cargoPath = fromRoot("apps/desktop/src-tauri/Cargo.toml");
  const cargo = fs.readFileSync(cargoPath, "utf8");
  const updatedCargo = cargo.replace(
    /(^\[package\][\s\S]*?^version\s*=\s*)"[^"]+"/m,
    `$1"${next}"`,
  );
  if (updatedCargo === cargo) {
    throw new Error("Could not locate the package version in Cargo.toml");
  }
  fs.writeFileSync(cargoPath, updatedCargo);
  console.log(`Updated Quill ${current} → ${next}`);
  process.exit(0);
}

for (const relativePath of jsonPaths) {
  const actual = JSON.parse(fs.readFileSync(fromRoot(relativePath), "utf8")).version;
  if (actual !== next) {
    throw new Error(`${relativePath} has ${actual}, expected ${next}`);
  }
}

const cargo = fs.readFileSync(fromRoot("apps/desktop/src-tauri/Cargo.toml"), "utf8");
if (!cargo.match(new RegExp(`^version = "${next.replaceAll(".", "\\.")}"$`, "m"))) {
  throw new Error("Cargo.toml version was not updated");
}

const lock = fs.readFileSync(fromRoot("apps/desktop/src-tauri/Cargo.lock"), "utf8");
const quillPackage = lock
  .split("[[package]]")
  .find((block) => block.includes('\nname = "quill"\n'));
if (!quillPackage?.includes(`\nversion = "${next}"\n`)) {
  throw new Error("Cargo.lock Quill package version was not updated");
}

console.log(`Verified Quill ${next} in every version source`);
