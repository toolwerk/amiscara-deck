// Builds daemon, hook program and command line of the workspace and puts them
// where Tauri takes its external programs from: src-tauri/binaries/<name>-<target>.
// The installer then lays them next to the interface.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const out = join(here, "..", "src-tauri", "binaries");
const target = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((l) => l.startsWith("host: "))
  .slice(6)
  .trim();
const ext = process.platform === "win32" ? ".exe" : "";

execFileSync(
  "cargo",
  ["build", "--release", "--locked", "-p", "deck-daemon", "-p", "deck-hook", "-p", "deck-cli"],
  { cwd: root, stdio: "inherit" },
);
mkdirSync(out, { recursive: true });
for (const name of ["amiscara-deckd", "deck-hook", "amiscara-deck"]) {
  copyFileSync(join(root, "target", "release", name + ext), join(out, `${name}-${target}${ext}`));
}
console.log(`helpers for ${target} in ${out}`);
