//! Offline generator: derives herdr's 16-token UI palette from the Ghostty
//! theme files vendored from iTerm2-Color-Schemes, and writes the result to
//! `src/app/generated_themes.rs`.
//!
//! Run via `just gen-themes [dir]`. See `vendor/ghostty-themes.vendor.json`
//! for the source pin.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

const DEFAULT_INPUT_DIR: &str = "/private/tmp/claude-501/-Volumes-KhaiSSD-Documents-Github-personal-herdr/adb4e5f3-0999-4ef0-af68-47afb4563ab7/scratchpad/ghostty-themes/ghostty";

/// herdr's curated theme slugs. Any generated theme colliding with one of
/// these is excluded so the hand-authored curated version wins.
const CURATED_NAMES: &[&str] = &[
    "catppuccin",
    "catppuccin-latte",
    "terminal",
    "tokyo-night",
    "tokyo-night-day",
    "dracula",
    "nord",
    "gruvbox",
    "gruvbox-light",
    "one-dark",
    "one-light",
    "solarized",
    "solarized-light",
    "kanagawa",
    "kanagawa-lotus",
    "rose-pine",
    "rose-pine-dawn",
    "vesper",
];

type Rgb = (u8, u8, u8);

struct ParsedTheme {
    palette: [Rgb; 16],
    bg: Rgb,
    fg: Rgb,
}

struct DerivedTheme {
    slug: String,
    is_dark: bool,
    colors: [u32; 16],
}

fn parse_hex(hex: &str) -> Rgb {
    let hex = hex.trim().trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).expect("invalid hex red channel");
    let g = u8::from_str_radix(&hex[2..4], 16).expect("invalid hex green channel");
    let b = u8::from_str_radix(&hex[4..6], 16).expect("invalid hex blue channel");
    (r, g, b)
}

fn parse_theme_file(content: &str, filename: &str) -> ParsedTheme {
    let mut palette: [Option<Rgb>; 16] = [None; 16];
    let mut bg: Option<Rgb> = None;
    let mut fg: Option<Rgb> = None;

    for line in content.lines() {
        let parts: Vec<&str> = line
            .split('=')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            continue;
        }
        match parts[0] {
            "palette" if parts.len() == 3 => {
                let idx: usize = parts[1]
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid palette index in {filename}: {line}"));
                if idx < 16 {
                    palette[idx] = Some(parse_hex(parts[2]));
                }
            }
            "background" if parts.len() == 2 => bg = Some(parse_hex(parts[1])),
            "foreground" if parts.len() == 2 => fg = Some(parse_hex(parts[1])),
            _ => {}
        }
    }

    let mut resolved_palette = [(0u8, 0u8, 0u8); 16];
    for (i, slot) in palette.iter().enumerate() {
        resolved_palette[i] = slot.unwrap_or_else(|| panic!("missing palette[{i}] in {filename}"));
    }

    ParsedTheme {
        palette: resolved_palette,
        bg: bg.unwrap_or_else(|| panic!("missing background in {filename}")),
        fg: fg.unwrap_or_else(|| panic!("missing foreground in {filename}")),
    }
}

fn lum((r, g, b): Rgb) -> f64 {
    0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64
}

fn lerp(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let ch = |a: u8, b: u8| -> u8 {
        (a as f64 + (b as f64 - a as f64) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2))
}

fn pack((r, g, b): Rgb) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | (b as u32)
}

/// Derives herdr's 16-token UI palette from a parsed Ghostty theme, in
/// Palette-token order (see `derivation_order` doc comment in the emitted
/// generated file).
fn derive_tokens(t: &ParsedTheme) -> ([Rgb; 16], bool) {
    let ansi = t.palette;
    let bg = t.bg;
    let fg = t.fg;

    let accent = ansi[4];
    let panel_bg = bg;

    // ANSI-8 (bright black) is the canonical muted-surface tone. If it's too
    // close to the background to give any elevation, fabricate the anchor
    // from fg instead. selection-background is an unreliable anchor because
    // many themes set it to a light accent color regardless of bg/fg.
    let d_lum = (lum(ansi[8]) - lum(bg)).abs();
    let anchor = if d_lum < 24.0 {
        lerp(bg, fg, 0.30)
    } else {
        ansi[8]
    };

    let surface_dim = lerp(bg, anchor, 0.10);
    let surface0 = lerp(bg, anchor, 0.40);
    let surface1 = lerp(bg, anchor, 0.70);
    let overlay0 = lerp(anchor, fg, 0.17);
    let overlay1 = lerp(anchor, fg, 0.34);
    let text = fg;
    let subtext0 = lerp(anchor, fg, 0.67);
    let mauve = ansi[5];
    let green = ansi[2];
    let yellow = ansi[3];
    let red = ansi[1];
    let blue = ansi[4];
    let teal = ansi[6];
    let peach = lerp(ansi[1], ansi[3], 0.5);

    let is_dark = lum(bg) < 128.0;

    (
        [
            accent,
            panel_bg,
            surface0,
            surface1,
            surface_dim,
            overlay0,
            overlay1,
            text,
            subtext0,
            mauve,
            green,
            yellow,
            red,
            blue,
            teal,
            peach,
        ],
        is_dark,
    )
}

/// Slug rules: lowercase; `+` -> `-plus`; every run of non `[a-z0-9]` becomes
/// a single `-`; trim leading/trailing `-`.
fn slugify(name: &str) -> String {
    let lower = name.to_lowercase();
    let replaced = lower.replace('+', "-plus");
    let mut out = String::new();
    let mut last_dash = false;
    for c in replaced.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn main() {
    let dir = env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_INPUT_DIR.to_string());
    let dir_path = Path::new(&dir);

    let mut entries: Vec<_> = fs::read_dir(dir_path)
        .unwrap_or_else(|e| panic!("failed to read input dir {}: {e}", dir))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut parsed_count = 0usize;
    let mut excluded_count = 0usize;
    let mut slug_counts: HashMap<String, u32> = HashMap::new();
    let mut results: Vec<DerivedTheme> = Vec::new();

    for entry in &entries {
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read theme file {filename}: {e}"));
        parsed_count += 1;

        let parsed = parse_theme_file(&content, &filename);
        let raw_slug = slugify(&filename);

        if CURATED_NAMES.contains(&raw_slug.as_str()) {
            excluded_count += 1;
            continue;
        }

        let count = slug_counts.entry(raw_slug.clone()).or_insert(0);
        *count += 1;
        let slug = if *count == 1 {
            raw_slug
        } else {
            format!("{raw_slug}-{count}")
        };

        let (colors, is_dark) = derive_tokens(&parsed);
        results.push(DerivedTheme {
            slug,
            is_dark,
            colors: colors.map(pack),
        });
    }

    results.sort_by(|a, b| a.slug.cmp(&b.slug));

    let written = results.len();
    write_output(&results);

    println!("gen_themes: parsed {parsed_count}, excluded {excluded_count}, written {written}");
}

fn write_output(themes: &[DerivedTheme]) {
    let mut out = String::new();
    out.push_str(
        "// @generated by `just gen-themes` from iTerm2-Color-Schemes 97e244cf. Do not edit by hand.\n\n",
    );
    out.push_str("pub struct GeneratedTheme {\n");
    out.push_str("    pub slug: &'static str,\n");
    // Not read outside this module's own tests today; kept as part of the
    // generated data contract for future dark/light theme grouping.
    out.push_str("    #[allow(dead_code)]\n");
    out.push_str("    pub is_dark: bool,\n");
    out.push_str("    /// 16 tokens, packed 0xRRGGBB, in Palette-token order:\n");
    out.push_str(
        "    /// accent, panel_bg, surface0, surface1, surface_dim, overlay0, overlay1,\n",
    );
    out.push_str("    /// text, subtext0, mauve, green, yellow, red, blue, teal, peach\n");
    out.push_str("    pub colors: [u32; 16],\n");
    out.push_str("}\n\n");

    out.push_str("pub static GENERATED_THEMES: &[GeneratedTheme] = &[\n");
    for t in themes {
        out.push_str("    GeneratedTheme {\n");
        out.push_str(&format!("        slug: \"{}\",\n", t.slug));
        out.push_str(&format!("        is_dark: {},\n", t.is_dark));
        let colors_str = t
            .colors
            .iter()
            .map(|c| format!("0x{c:06x}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("        colors: [{colors_str}],\n"));
        out.push_str("    },\n");
    }
    out.push_str("];\n\n");

    out.push_str("#[cfg(test)]\n");
    out.push_str("mod tests {\n");
    out.push_str("    use super::*;\n\n");
    out.push_str("    const CURATED_NAMES: &[&str] = &[\n");
    out.push_str("        \"catppuccin\",\n");
    out.push_str("        \"catppuccin-latte\",\n");
    out.push_str("        \"terminal\",\n");
    out.push_str("        \"tokyo-night\",\n");
    out.push_str("        \"tokyo-night-day\",\n");
    out.push_str("        \"dracula\",\n");
    out.push_str("        \"nord\",\n");
    out.push_str("        \"gruvbox\",\n");
    out.push_str("        \"gruvbox-light\",\n");
    out.push_str("        \"one-dark\",\n");
    out.push_str("        \"one-light\",\n");
    out.push_str("        \"solarized\",\n");
    out.push_str("        \"solarized-light\",\n");
    out.push_str("        \"kanagawa\",\n");
    out.push_str("        \"kanagawa-lotus\",\n");
    out.push_str("        \"rose-pine\",\n");
    out.push_str("        \"rose-pine-dawn\",\n");
    out.push_str("        \"vesper\",\n");
    out.push_str("    ];\n\n");

    out.push_str("    #[test]\n");
    out.push_str("    fn theme_count_in_range() {\n");
    out.push_str("        let len = GENERATED_THEMES.len();\n");
    out.push_str(
        "        assert!((550..=592).contains(&len), \"unexpected theme count: {len}\");\n",
    );
    out.push_str("    }\n\n");

    out.push_str("    #[test]\n");
    out.push_str("    fn slugs_are_ascending_and_unique() {\n");
    out.push_str("        for pair in GENERATED_THEMES.windows(2) {\n");
    out.push_str("            assert!(\n");
    out.push_str("                pair[0].slug < pair[1].slug,\n");
    out.push_str("                \"slugs not strictly ascending: {} >= {}\",\n");
    out.push_str("                pair[0].slug,\n");
    out.push_str("                pair[1].slug\n");
    out.push_str("            );\n");
    out.push_str("        }\n");
    out.push_str("    }\n\n");

    out.push_str("    #[test]\n");
    out.push_str("    fn no_curated_name_collisions() {\n");
    out.push_str("        for theme in GENERATED_THEMES {\n");
    out.push_str("            assert!(\n");
    out.push_str("                !CURATED_NAMES.contains(&theme.slug),\n");
    out.push_str("                \"generated slug collides with curated name: {}\",\n");
    out.push_str("                theme.slug\n");
    out.push_str("            );\n");
    out.push_str("        }\n");
    out.push_str("    }\n\n");

    out.push_str("    #[test]\n");
    out.push_str("    fn has_both_dark_and_light_themes() {\n");
    out.push_str("        assert!(GENERATED_THEMES.iter().any(|t| t.is_dark));\n");
    out.push_str("        assert!(GENERATED_THEMES.iter().any(|t| !t.is_dark));\n");
    out.push_str("    }\n");
    out.push_str("}\n");

    let out_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app/generated_themes.rs");
    fs::write(&out_path, out)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));
}
