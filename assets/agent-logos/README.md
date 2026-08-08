# Agent logo masks

Each `*.mask` is a raw 96x96 8-bit alpha coverage map (9216 bytes, one byte per
pixel, row-major, no header). herdr tints a mask with the agent's brand color at
render time and ships it to the host terminal as raw RGBA over the Kitty
graphics protocol, so no image decoder is needed at runtime.

## Provenance and license conflict

These masks are rasterized from the monochrome agent silhouettes bundled with
Warp (`app/assets/bundled/svg/` in `warpdotdev/warp`). **Warp is licensed
AGPL-3.0 / MIT; herdr is Apache-2.0.** Redistributing assets derived from the
AGPL tree under Apache-2.0 is a license conflict.

That is acceptable for a private fork and is a blocker for contributing this
feature upstream. Anyone preparing an upstream pull request must first replace
every mask here with one rasterized from the vendor's own published brand assets
(or drop the logo and fall back to the brand-colored text token), and update this
file to record the new source per mask.

The underlying marks are third-party trademarks belonging to their respective
owners and are used here only to identify the corresponding tool.

## Regenerating

The masks come from SVG via `scripts/rasterize_agent_logos.js`:

```bash
bun add @resvg/resvg-js
bun run scripts/rasterize_agent_logos.js <svg-dir> assets/agent-logos
```

The script forces every fill opaque, rasterizes at 96x96, and keeps only the
alpha channel. It fails if any mask rasterizes empty.

## Coverage

Masks exist for 12 of the 21 variants in `detect::Agent`:

| mask | `detect::Agent` | source |
| --- | --- | --- |
| `amp` | `Amp` | Warp monochrome silhouette |
| `antigravity` | `Antigravity` | Warp monochrome silhouette |
| `claude` | `Claude` | Warp monochrome silhouette |
| `codex` | `Codex` | Warp monochrome silhouette |
| `cursor` | `Cursor` | Warp monochrome silhouette |
| `droid` | `Droid` | Warp monochrome silhouette |
| `gemini` | `Gemini` | Warp monochrome silhouette |
| `githubcopilot` | `GithubCopilot` | Warp monochrome silhouette |
| `grok` | `Grok` | `website/assets/agent-icons/grok.svg` (xAI mark) |
| `omp` | `Omp` | Warp monochrome silhouette |
| `opencode` | `OpenCode` | Warp monochrome silhouette |
| `pi` | `Pi` | Warp monochrome silhouette |

The remaining agents — `Cline`, `Devin`, `Hermes`, `Kilo`, `Kimi`, `Kiro`,
`Maki`, `Mastracode`, `Qodercli` — have no mask and fall back to the
brand-colored text token. Adding a mask named after the lowercased variant is
enough to light one up; see `agent_logo_mask` in `src/ui/sidebar/agent_logo.rs`.
