import { spawnSync } from "node:child_process";

const candidates =
  process.platform === "win32"
    ? [
        ["python", []],
        ["py", ["-3"]],
        ["python3", []],
      ]
    : [
        ["python3", []],
        ["python", []],
      ];

for (const [command, prefix] of candidates) {
  const result = spawnSync(
    command,
    [...prefix, "prototypes/test_scribe_buffer.py"],
    { stdio: "inherit" },
  );

  if (result.error?.code === "ENOENT") continue;
  process.exit(result.status ?? 1);
}

console.error("Python 3 was not found. Install it to run the Scribe prototype tests.");
process.exit(1);
