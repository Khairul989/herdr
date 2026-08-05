// Rasterize Warp's monochrome agent SVGs into 8-bit alpha masks for herdr.
//
// The SVGs are single-color silhouettes (Warp paints them at runtime), so we
// keep only coverage: one byte of alpha per pixel. herdr tints the mask with
// the agent's brand color and ships it as raw RGBA over the Kitty protocol.
//
// Usage: bun run rasterize_logos.js <warp-svg-dir> <out-dir>

import { Resvg } from "@resvg/resvg-js";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const SIZE = 96; // square; herdr scales to the host cell box at placement time

// warp asset basename -> herdr detect::Agent variant
const MAP = {
  claude: "Claude",
  openai: "Codex",
  gemini_cli: "Gemini",
  copilot: "GithubCopilot",
  cursor: "Cursor",
  opencode: "OpenCode",
  amp: "Amp",
  droid: "Droid",
  pi: "Pi",
  oh_my_pi: "Omp",
  antigravity_cli: "Antigravity",
};

const [srcDir, outDir] = process.argv.slice(2);
if (!srcDir || !outDir) {
  console.error("usage: bun run rasterize_logos.js <warp-svg-dir> <out-dir>");
  process.exit(1);
}
mkdirSync(outDir, { recursive: true });

for (const [basename, agent] of Object.entries(MAP)) {
  const svg = readFileSync(join(srcDir, `${basename}.svg`), "utf8");
  // Warp's tint placeholder is #FF0000 plus white knockouts. Force every fill
  // opaque black so coverage is purely geometric, then read back the alpha.
  const normalized = svg
    .replaceAll('fill="#FF0000"', 'fill="#000000"')
    .replaceAll('fill="white"', 'fill="#000000"');

  const png = new Resvg(normalized, {
    fitTo: { mode: "width", value: SIZE },
    background: "rgba(0,0,0,0)",
  }).render();

  const { width, height } = png;
  const rgba = png.pixels;
  const mask = Buffer.alloc(width * height);
  for (let i = 0; i < width * height; i++) mask[i] = rgba[i * 4 + 3];

  const nonEmpty = mask.reduce((n, a) => n + (a > 0 ? 1 : 0), 0);
  const outFile = join(outDir, `${agent.toLowerCase()}.mask`);
  writeFileSync(outFile, mask);
  console.log(
    `${agent.padEnd(14)} ${width}x${height}  ${nonEmpty} covered px ` +
      `(${((nonEmpty / (width * height)) * 100).toFixed(1)}%)  -> ${outFile}`,
  );
  if (nonEmpty === 0) {
    console.error(`  !! ${agent} rasterized to an EMPTY mask`);
    process.exitCode = 1;
  }
}
