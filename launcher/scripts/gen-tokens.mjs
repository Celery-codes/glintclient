// Brings the shared branding into the launcher: branding/tokens.json (also used by the
// client mod) becomes CSS variables, and the logo becomes the page icon.
// Runs before `dev` and `build`; edit the files in branding/, not the generated copies.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const source = resolve(here, "../../branding/tokens.json");
const target = resolve(here, "../src/theme/tokens.css");

const tokens = JSON.parse(readFileSync(source, "utf8"));
const kebab = (name) => name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

const lines = [];
for (const [name, value] of Object.entries(tokens.color)) {
  lines.push(`  --glint-color-${kebab(name)}: ${value};`);
}
for (const [name, value] of Object.entries(tokens.radius)) {
  lines.push(`  --glint-radius-${kebab(name)}: ${value}px;`);
}

mkdirSync(dirname(target), { recursive: true });
writeFileSync(
  target,
  `/* Generated from branding/tokens.json by scripts/gen-tokens.mjs. Do not edit. */\n:root {\n${lines.join("\n")}\n}\n`,
);

const publicDir = resolve(here, "../public");
mkdirSync(publicDir, { recursive: true });
copyFileSync(resolve(here, "../../branding/logo.svg"), resolve(publicDir, "logo.svg"));
