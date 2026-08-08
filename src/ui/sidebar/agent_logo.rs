//! Agent brand logos for the sidebar, drawn as Kitty graphics.
//!
//! Each logo ships as a raw 8-bit alpha coverage mask (see
//! `assets/agent-logos/README.md`). A mask carries shape only, so it is tinted
//! at render time with the same brand color the `agent` text token uses and
//! handed to the host terminal as raw RGBA. That keeps a logo consistent with
//! its label, lets it follow the theme for agents with no fixed brand color,
//! and avoids pulling an image decoder into the binary.
//!
//! Rendering is a two-step Kitty exchange: transmit the tinted image once per
//! client and then re-place it cheaply on every frame that needs it. The
//! per-client cache in `kitty_graphics` remembers which step already happened.

use ratatui::style::Color;

use crate::app::state::Palette;
use crate::detect::Agent;

/// Masks are square and stored uncompressed, one alpha byte per pixel.
pub(crate) const MASK_DIM: u32 = 96;
#[cfg(test)]
const MASK_LEN: usize = (MASK_DIM * MASK_DIM) as usize;

/// Cell columns a logo occupies. One row tall, so it sits on the agent's own
/// line without changing entry height.
pub(crate) const LOGO_COLS: u16 = 2;

/// Kitty image ids reserved for agent logos.
///
/// Pane graphics allocate from [`HOST_IMAGE_ID_BASE`](super::super::kitty_graphics)
/// (10_000) upward, or with the high bit set for streamed pane graphics, so this
/// low range cannot collide with them.
pub(crate) const LOGO_IMAGE_ID_BASE: u32 = 1_000;

macro_rules! masks {
    ($($agent:ident => $file:literal),* $(,)?) => {
        /// Alpha mask for `agent`, or `None` when no logo is bundled.
        pub(crate) fn agent_logo_mask(agent: Agent) -> Option<&'static [u8]> {
            match agent {
                $(Agent::$agent => Some(include_bytes!(concat!(
                    "../../../assets/agent-logos/", $file, ".mask"
                ))),)*
                _ => None,
            }
        }

        #[cfg(test)]
        const AGENTS_WITH_MASKS: &[Agent] = &[$(Agent::$agent),*];
    };
}

masks! {
    Amp => "amp",
    Antigravity => "antigravity",
    Claude => "claude",
    Codex => "codex",
    Cursor => "cursor",
    Droid => "droid",
    Gemini => "gemini",
    GithubCopilot => "githubcopilot",
    Grok => "grok",
    Omp => "omp",
    OpenCode => "opencode",
    Pi => "pi",
}

/// Stable Kitty image id for an agent's logo.
///
/// Keyed off the agent's position in [`Agent::ALL`] so the id survives across
/// frames and clients; a reordering of that array changes ids, which only costs
/// one retransmit.
pub(crate) fn logo_image_id(agent: Agent) -> u32 {
    let index = Agent::ALL
        .iter()
        .position(|candidate| *candidate == agent)
        .unwrap_or(0) as u32;
    LOGO_IMAGE_ID_BASE + index
}

/// The brand color for an agent, or `None` when the agent has no fixed brand
/// color and should take the theme's foreground.
///
/// Single source of truth: both the `agent` text token's style and the logo
/// tint derive from this, so a label and its logo can never disagree.
pub(crate) fn agent_brand_color(agent: Agent) -> Option<Color> {
    let rgb = match agent {
        Agent::Claude => (0xD9, 0x77, 0x57),
        Agent::Codex => (0x10, 0xA3, 0x7F),
        Agent::Gemini | Agent::Antigravity => (0x42, 0x85, 0xF4),
        Agent::GithubCopilot => (0x89, 0x57, 0xE5),
        Agent::Devin => (0x2E, 0x6C, 0xF6),
        Agent::Kiro => (0x79, 0x0E, 0xF7),
        Agent::Amp => (0xFF, 0x55, 0x43),
        _ => return None,
    };
    Some(Color::Rgb(rgb.0, rgb.1, rgb.2))
}

/// Concrete RGB used to tint a logo.
///
/// Falls back to the palette foreground for unbranded agents, and to a neutral
/// light gray when the palette color is not a direct RGB value (an indexed or
/// `Reset` color has no components to tint with).
pub(crate) fn logo_tint(agent: Agent, p: &Palette) -> (u8, u8, u8) {
    let color = agent_brand_color(agent).unwrap_or(p.text);
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0xC0, 0xC0, 0xC0),
    }
}

/// Expand an alpha mask into straight-alpha RGBA at the mask's native size.
pub(crate) fn tint_mask(mask: &[u8], (r, g, b): (u8, u8, u8)) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(mask.len() * 4);
    for alpha in mask {
        rgba.extend_from_slice(&[r, g, b, *alpha]);
    }
    rgba
}

/// Scale the square mask to fit a `target_w` x `target_h` pixel box, preserving
/// aspect ratio and padding the remainder with transparent pixels.
///
/// The box must be the exact pixel size of the cell rectangle the logo will
/// occupy. Kitty scales an image to *fill* the `c` x `r` cell box it is placed
/// in, so an image built at any other size gets stretched back out — letterboxing
/// inside a wrongly sized buffer accomplishes nothing. Sizing the buffer to the
/// real cell box makes that scale a 1:1 copy, which is the only way the aspect
/// ratio actually survives.
pub(crate) fn fit_mask_to_box(mask: &[u8], target_w: u32, target_h: u32) -> Vec<u8> {
    if target_w == 0 || target_h == 0 {
        return Vec::new();
    }
    let side = target_w.min(target_h);
    let x_pad = (target_w - side) / 2;
    let y_pad = (target_h - side) / 2;
    let mut out = vec![0u8; (target_w * target_h) as usize];

    for y in 0..side {
        // Box filter: average every source pixel that lands in this destination
        // pixel. Nearest-neighbour drops thin strokes at these sizes.
        let sy0 = y * MASK_DIM / side;
        let sy1 = ((y + 1) * MASK_DIM / side).max(sy0 + 1);
        for x in 0..side {
            let sx0 = x * MASK_DIM / side;
            let sx1 = ((x + 1) * MASK_DIM / side).max(sx0 + 1);
            let mut total = 0u32;
            let mut count = 0u32;
            for sy in sy0..sy1 {
                let row = (sy * MASK_DIM) as usize;
                for sx in sx0..sx1 {
                    total += u32::from(mask[row + sx as usize]);
                    count += 1;
                }
            }
            let dst = ((y + y_pad) * target_w + (x + x_pad)) as usize;
            out[dst] = (total / count.max(1)) as u8;
        }
    }
    out
}

/// Fingerprint of a tint, used to notice a theme change that requires
/// retransmitting an already-sent logo.
pub(crate) fn tint_fingerprint((r, g, b): (u8, u8, u8)) -> u32 {
    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}

/// One logo to draw, resolved to host cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentLogoPlacement {
    pub agent: Agent,
    /// Zero-based host terminal column of the logo's left edge.
    pub col: u16,
    /// Zero-based host terminal row.
    pub row: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_mask_has_the_expected_length() {
        for agent in AGENTS_WITH_MASKS {
            let mask = agent_logo_mask(*agent).expect("bundled agent must have a mask");
            assert_eq!(
                mask.len(),
                MASK_LEN,
                "{agent:?} mask is {} bytes, expected {MASK_LEN}",
                mask.len()
            );
        }
    }

    #[test]
    fn every_bundled_mask_has_visible_coverage() {
        // A mask that rasterized empty would render as an invisible logo and
        // silently look like the feature is off.
        for agent in AGENTS_WITH_MASKS {
            let mask = agent_logo_mask(*agent).expect("bundled agent must have a mask");
            let covered = mask.iter().filter(|alpha| **alpha > 0).count();
            assert!(
                covered > MASK_LEN / 100,
                "{agent:?} mask covers only {covered} of {MASK_LEN} pixels"
            );
        }
    }

    #[test]
    fn agents_without_a_bundled_mask_report_none() {
        for agent in Agent::ALL {
            let bundled = AGENTS_WITH_MASKS.contains(&agent);
            assert_eq!(
                agent_logo_mask(agent).is_some(),
                bundled,
                "{agent:?} mask presence disagrees with the bundled list"
            );
        }
    }

    #[test]
    fn logo_image_ids_are_unique_per_agent() {
        let mut ids: Vec<u32> = Agent::ALL.iter().copied().map(logo_image_id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "logo image ids collide across agents");
    }

    #[test]
    fn logo_image_ids_stay_below_the_pane_graphics_range() {
        // Pane graphics allocate from 10_000 upward; overlapping would make a
        // logo delete someone else's image.
        for agent in Agent::ALL {
            assert!(
                logo_image_id(agent) < 10_000,
                "{agent:?} logo id overlaps the pane graphics id range"
            );
        }
    }

    #[test]
    fn tinting_preserves_alpha_and_applies_color() {
        let mask = [0u8, 128, 255];
        let rgba = tint_mask(&mask, (0x11, 0x22, 0x33));
        assert_eq!(
            rgba,
            vec![
                0x11, 0x22, 0x33, 0, //
                0x11, 0x22, 0x33, 128, //
                0x11, 0x22, 0x33, 255,
            ]
        );
    }

    #[test]
    fn branded_agents_tint_with_their_brand_color() {
        let p = Palette::catppuccin();
        assert_eq!(logo_tint(Agent::Claude, &p), (0xD9, 0x77, 0x57));
        assert_eq!(logo_tint(Agent::Codex, &p), (0x10, 0xA3, 0x7F));
    }

    #[test]
    fn unbranded_agents_tint_with_the_palette_foreground() {
        let mut p = Palette::catppuccin();
        p.text = Color::Rgb(1, 2, 3);
        assert_eq!(agent_brand_color(Agent::OpenCode), None);
        assert_eq!(logo_tint(Agent::OpenCode, &p), (1, 2, 3));
    }

    #[test]
    fn a_non_rgb_palette_foreground_falls_back_to_gray() {
        let mut p = Palette::catppuccin();
        p.text = Color::Reset;
        assert_eq!(logo_tint(Agent::OpenCode, &p), (0xC0, 0xC0, 0xC0));
    }

    #[test]
    fn tint_fingerprint_distinguishes_colors() {
        assert_ne!(
            tint_fingerprint((0xD9, 0x77, 0x57)),
            tint_fingerprint((0x10, 0xA3, 0x7F))
        );
        assert_eq!(tint_fingerprint((0, 0, 0)), 0);
        assert_eq!(tint_fingerprint((0xFF, 0xFF, 0xFF)), 0x00FF_FFFF);
    }

    /// A fully opaque mask, so coverage after fitting is purely a question of
    /// geometry rather than of the source artwork.
    fn solid_mask() -> Vec<u8> {
        vec![0xFF; MASK_LEN]
    }

    #[test]
    fn fitting_produces_a_buffer_of_exactly_the_target_size() {
        let fitted = fit_mask_to_box(&solid_mask(), 20, 22);
        assert_eq!(fitted.len(), 20 * 22);
    }

    #[test]
    fn fitting_letterboxes_instead_of_stretching() {
        // A wide box must leave transparent columns on both sides rather than
        // smearing a square logo across the full width. This is the defect that
        // made 3x2 look stretched.
        let w = 30;
        let h = 10;
        let fitted = fit_mask_to_box(&solid_mask(), w, h);

        let opaque_in_row = |y: u32| {
            (0..w)
                .filter(|x| fitted[(y * w + x) as usize] > 0)
                .collect::<Vec<_>>()
        };
        let covered = opaque_in_row(h / 2);
        assert_eq!(
            covered.len() as u32,
            h,
            "logo should span only the short side ({h}px), not the full width"
        );
        assert_eq!(covered.first().copied(), Some((w - h) / 2), "not centered");
    }

    #[test]
    fn fitting_a_tall_box_pads_top_and_bottom() {
        let w = 10;
        let h = 30;
        let fitted = fit_mask_to_box(&solid_mask(), w, h);
        let opaque_in_col = |x: u32| {
            (0..h)
                .filter(|y| fitted[(y * w + x) as usize] > 0)
                .collect::<Vec<_>>()
        };
        let covered = opaque_in_col(w / 2);
        assert_eq!(
            covered.len() as u32,
            w,
            "logo should span only the short side"
        );
        assert_eq!(covered.first().copied(), Some((h - w) / 2), "not centered");
    }

    #[test]
    fn a_square_box_is_fully_covered_by_a_solid_mask() {
        let fitted = fit_mask_to_box(&solid_mask(), 24, 24);
        assert!(
            fitted.iter().all(|alpha| *alpha == 0xFF),
            "a square target should need no padding at all"
        );
    }

    #[test]
    fn fitting_preserves_real_artwork_coverage() {
        // Downscaling must not erase the logo: a box filter over a sparse mask
        // can round thin strokes away to nothing if the math is wrong.
        let mask = agent_logo_mask(Agent::Claude).expect("claude mask");
        let fitted = fit_mask_to_box(mask, 20, 22);
        let covered = fitted.iter().filter(|alpha| **alpha > 0).count();
        assert!(
            covered > 40,
            "claude logo nearly vanished when scaled down: {covered} covered pixels"
        );
    }

    #[test]
    fn fitting_a_degenerate_box_yields_nothing_rather_than_panicking() {
        assert!(fit_mask_to_box(&solid_mask(), 0, 10).is_empty());
        assert!(fit_mask_to_box(&solid_mask(), 10, 0).is_empty());
    }
}
