pub(crate) mod agent_logo;
mod tokens;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
};

pub(crate) use self::tokens::{
    agent_rows as sidebar_agent_rows, space_rows as sidebar_space_rows, AgentTokenContext,
    ResolvedToken, ResolvedTokenKind, SpaceTokenContext,
};
use super::text::{display_width, truncate_end};
use crate::app::state::Palette;
use crate::app::AppState;
use crate::detect::AgentState;
use crate::terminal::TerminalRuntimeRegistry;

pub(crate) struct AgentPanelEntry {
    pub ws_idx: usize,
    pub tab_idx: usize,
    pub pane_id: crate::layout::PaneId,
    pub agent_kind_label: Option<String>,
    pub state: AgentState,
    pub seen: bool,
    pub last_agent_state_change_seq: Option<u64>,
    pub tokens: std::collections::HashMap<String, String>,
}

fn sidebar_section_heights(total_height: u16, split_ratio: f32) -> (u16, u16) {
    if total_height == 0 {
        return (0, 0);
    }
    if total_height < 6 {
        let workspace_height = total_height.div_ceil(2);
        return (
            workspace_height,
            total_height.saturating_sub(workspace_height),
        );
    }

    let workspace_height = ((total_height as f32) * split_ratio.clamp(0.1, 0.9)).round() as u16;
    let workspace_height = workspace_height.clamp(3, total_height.saturating_sub(3));
    (
        workspace_height,
        total_height.saturating_sub(workspace_height),
    )
}

pub(crate) fn expanded_sidebar_sections(area: Rect, split_ratio: f32) -> (Rect, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.is_empty() {
        return (Rect::default(), Rect::default());
    }

    let (workspace_height, detail_height) = sidebar_section_heights(content.height, split_ratio);
    (
        Rect::new(content.x, content.y, content.width, workspace_height),
        Rect::new(
            content.x,
            content.y + workspace_height,
            content.width,
            detail_height,
        ),
    )
}

pub(crate) fn sidebar_section_divider_rect(area: Rect, split_ratio: f32) -> Rect {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height < 6 {
        return Rect::default();
    }

    let (workspace_height, _) = sidebar_section_heights(content.height, split_ratio);
    Rect::new(content.x, content.y + workspace_height, content.width, 1)
}

pub(crate) fn agent_panel_entries_from(
    app: &AppState,
    _terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelEntry> {
    let mut entries = app
        .workspaces
        .iter()
        .enumerate()
        .flat_map(|(ws_idx, workspace)| {
            workspace
                .pane_details(&app.terminals)
                .into_iter()
                .map(move |detail| AgentPanelEntry {
                    ws_idx,
                    tab_idx: detail.tab_idx,
                    pane_id: detail.pane_id,
                    agent_kind_label: detail.agent_kind_label,
                    state: detail.state,
                    seen: detail.seen,
                    last_agent_state_change_seq: detail.last_agent_state_change_seq,
                    tokens: detail.tokens,
                })
        })
        .collect();
    crate::app::agent_view::apply_agent_view(app, &mut entries);
    entries
}

/// Result of [`resolved_token_spans`]: the rendered spans, plus — when a real
/// logo image will be drawn — the 0-based column (relative to the start of
/// `spans`) it should be placed at.
pub(crate) struct ResolvedTokenLine {
    pub(crate) spans: Vec<Span<'static>>,
    pub(crate) logo_column: Option<u16>,
}

pub(crate) fn resolved_token_spans(
    resolved: &[ResolvedToken],
    state_icon: (&str, Style),
    state_text_style: Style,
    workspace_style: Style,
    secondary_style: Style,
    custom_style: Style,
    palette: &Palette,
    max_width: usize,
    agent: Option<crate::detect::Agent>,
    logo_will_render: bool,
) -> ResolvedTokenLine {
    // Whether this row prints the agent name through its own `agent` token.
    // An `agent_icon` token whose logo cannot be drawn degrades to the same
    // name; when the row already carries `agent` it elides instead of
    // printing the name twice. A real logo is not text, so it never elides
    // this way even when `agent` is also present.
    let row_names_agent = resolved
        .iter()
        .any(|token| matches!(token.kind, ResolvedTokenKind::Agent(_)));
    let fixed_widths = resolved
        .iter()
        .map(|token| match &token.kind {
            ResolvedTokenKind::StateIcon => display_width(state_icon.0),
            ResolvedTokenKind::GitStatus { ahead, behind } => {
                usize::from(*ahead > 0) * display_width(&format!("↑{ahead}"))
                    + usize::from(*behind > 0) * display_width(&format!("↓{behind}"))
                    + usize::from(*ahead > 0 && *behind > 0)
            }
            // A real logo is a fixed-size image, not squeezable/elidable text.
            ResolvedTokenKind::AgentIcon { .. } if logo_will_render => {
                agent_logo::LOGO_COLS as usize
            }
            _ => 0,
        })
        .collect::<Vec<_>>();
    let flexible_widths = resolved
        .iter()
        .map(|token| match &token.kind {
            // Already reserved as a fixed width above.
            ResolvedTokenKind::AgentIcon { .. } if logo_will_render => 0,
            // Elides entirely: the row's own `agent` token prints the name.
            ResolvedTokenKind::AgentIcon { .. } if row_names_agent => 0,
            ResolvedTokenKind::StateText(text)
            | ResolvedTokenKind::Machine(text)
            | ResolvedTokenKind::Workspace(text)
            | ResolvedTokenKind::Tab(text)
            | ResolvedTokenKind::Pane(text)
            | ResolvedTokenKind::Agent(text)
            | ResolvedTokenKind::AgentIcon { fallback: text }
            | ResolvedTokenKind::TerminalTitle(text)
            | ResolvedTokenKind::Branch(text)
            | ResolvedTokenKind::Custom(text) => display_width(text),
            _ => 0,
        })
        .collect::<Vec<_>>();
    let minimum_width = |active: &[bool]| {
        let indices = active
            .iter()
            .enumerate()
            .filter_map(|(index, active)| active.then_some(index))
            .collect::<Vec<_>>();
        let content = indices
            .iter()
            .map(|index| fixed_widths[*index] + usize::from(flexible_widths[*index] > 0))
            .sum::<usize>();
        let separators = indices
            .windows(2)
            .map(|pair| display_width(tokens::separator(&resolved[pair[0]], &resolved[pair[1]])))
            .sum::<usize>();
        content + separators
    };
    // An `agent_icon` elided by `row_names_agent` carries no content (see
    // above), so it must never occupy a `visible_indices` slot either —
    // otherwise it would still draw a separator into a position with
    // nothing after it, leaving a dangling gap. A real logo is never elided
    // this way.
    let mut active = resolved
        .iter()
        .map(|token| {
            !(row_names_agent
                && !logo_will_render
                && matches!(token.kind, ResolvedTokenKind::AgentIcon { .. }))
        })
        .collect::<Vec<_>>();
    if minimum_width(&active) > max_width {
        for (index, width) in flexible_widths.iter().enumerate() {
            if *width > 0 {
                active[index] = false;
            }
        }
        for index in (0..resolved.len()).rev() {
            if flexible_widths[index] == 0 {
                continue;
            }
            active[index] = true;
            if minimum_width(&active) > max_width {
                active[index] = false;
            }
        }
    }
    let visible_indices = active
        .iter()
        .enumerate()
        .filter_map(|(index, active)| active.then_some(index))
        .collect::<Vec<_>>();
    let separator_width = visible_indices
        .windows(2)
        .map(|pair| display_width(tokens::separator(&resolved[pair[0]], &resolved[pair[1]])))
        .sum::<usize>();
    let fixed_width = visible_indices
        .iter()
        .map(|index| fixed_widths[*index])
        .sum::<usize>();
    let mut budgets = flexible_widths
        .iter()
        .enumerate()
        .map(|(index, width)| usize::from(active[index] && *width > 0))
        .collect::<Vec<_>>();
    let minimum = budgets.iter().sum::<usize>();
    let mut remaining = max_width
        .saturating_sub(separator_width + fixed_width)
        .saturating_sub(minimum);
    while remaining > 0 {
        let mut grew = false;
        for (budget, width) in budgets.iter_mut().zip(&flexible_widths) {
            if *budget > 0 && *budget < *width {
                *budget += 1;
                remaining -= 1;
                grew = true;
                if remaining == 0 {
                    break;
                }
            }
        }
        if !grew {
            break;
        }
    }

    let mut spans = Vec::new();
    let mut column: u16 = 0;
    let mut logo_column: Option<u16> = None;
    for (position, index) in visible_indices.iter().copied().enumerate() {
        let token = &resolved[index];
        if position > 0 {
            let previous = &resolved[visible_indices[position - 1]];
            let separator = tokens::separator(previous, token);
            column = column.saturating_add(display_width(separator) as u16);
            spans.push(Span::styled(
                separator,
                Style::default()
                    .fg(palette.overlay0)
                    .add_modifier(Modifier::DIM),
            ));
        }
        match &token.kind {
            ResolvedTokenKind::StateIcon => {
                column = column.saturating_add(display_width(state_icon.0) as u16);
                spans.push(Span::styled(
                    state_icon.0.to_string(),
                    apply_token_style(state_icon.1, token.style),
                ));
            }
            ResolvedTokenKind::StateText(text) => {
                let text = truncate_end(text, budgets[index]);
                column = column.saturating_add(display_width(&text) as u16);
                spans.push(Span::styled(
                    text,
                    apply_token_style(state_text_style, token.style),
                ));
            }
            ResolvedTokenKind::Workspace(text) => {
                let text = truncate_end(text, budgets[index]);
                column = column.saturating_add(display_width(&text) as u16);
                spans.push(Span::styled(
                    text,
                    apply_token_style(workspace_style, token.style),
                ));
            }
            ResolvedTokenKind::Tab(text) => {
                let text = truncate_end(text, budgets[index]);
                column = column.saturating_add(display_width(&text) as u16);
                spans.push(Span::styled(
                    text,
                    apply_token_style(Style::default().fg(palette.teal), token.style),
                ));
            }
            ResolvedTokenKind::Agent(text) => {
                let base = agent
                    .and_then(|agent| agent_brand_style(agent, palette))
                    .unwrap_or(secondary_style);
                let text = truncate_end(text, budgets[index]);
                column = column.saturating_add(display_width(&text) as u16);
                spans.push(Span::styled(text, apply_token_style(base, token.style)));
            }
            ResolvedTokenKind::AgentIcon { fallback } => {
                if logo_will_render {
                    // Reserve the box for the Kitty image the caller will
                    // place at `logo_column`; a blank placeholder keeps the
                    // layout correct even for the one frame before the image
                    // lands, and is fully covered once it does.
                    logo_column = Some(column);
                    spans.push(Span::raw(" ".repeat(agent_logo::LOGO_COLS as usize)));
                    column = column.saturating_add(agent_logo::LOGO_COLS);
                } else if !row_names_agent {
                    // No Kitty logo path is available for this row (disabled,
                    // unsupported terminal, or no bundled mask), so this
                    // falls back to the name — unless the row already prints
                    // it via its own `agent` token, in which case the icon
                    // simply elides rather than repeating it.
                    let base = agent
                        .and_then(|agent| agent_brand_style(agent, palette))
                        .unwrap_or(secondary_style);
                    let text = truncate_end(fallback, budgets[index]);
                    column = column.saturating_add(display_width(&text) as u16);
                    spans.push(Span::styled(text, apply_token_style(base, token.style)));
                }
            }
            ResolvedTokenKind::Machine(text)
            | ResolvedTokenKind::Pane(text)
            | ResolvedTokenKind::Branch(text) => {
                let text = truncate_end(text, budgets[index]);
                column = column.saturating_add(display_width(&text) as u16);
                spans.push(Span::styled(
                    text,
                    apply_token_style(secondary_style, token.style),
                ));
            }
            ResolvedTokenKind::GitStatus { ahead, behind } => {
                if *ahead > 0 {
                    let text = format!("↑{ahead}");
                    column = column.saturating_add(display_width(&text) as u16);
                    spans.push(Span::styled(
                        text,
                        apply_token_style(Style::default().fg(palette.green), token.style),
                    ));
                }
                if *ahead > 0 && *behind > 0 {
                    column = column.saturating_add(1);
                    spans.push(Span::styled(
                        " ",
                        apply_token_style(Style::default(), token.style),
                    ));
                }
                if *behind > 0 {
                    let text = format!("↓{behind}");
                    column = column.saturating_add(display_width(&text) as u16);
                    spans.push(Span::styled(
                        text,
                        apply_token_style(Style::default().fg(palette.red), token.style),
                    ));
                }
            }
            ResolvedTokenKind::TerminalTitle(text) | ResolvedTokenKind::Custom(text) => {
                let text = truncate_end(text, budgets[index]);
                column = column.saturating_add(display_width(&text) as u16);
                spans.push(Span::styled(
                    text,
                    apply_token_style(custom_style, token.style),
                ));
            }
        }
    }
    ResolvedTokenLine { spans, logo_column }
}

/// Brand-recognition color for the agent name token and its logo. Delegates
/// fixed brand colors to [`agent_logo::agent_brand_color`] so the text token
/// and the logo tint can never disagree; brands with no color identity render
/// bold in the theme's text color, and agents without a verified brand
/// association return `None` to keep the default secondary styling.
/// Exhaustive on purpose: adding an `Agent` variant must decide its branding
/// here.
fn agent_brand_style(agent: crate::detect::Agent, p: &Palette) -> Option<Style> {
    use crate::detect::Agent;
    if let Some(color) = agent_logo::agent_brand_color(agent) {
        return Some(Style::default().fg(color));
    }
    match agent {
        Agent::Grok | Agent::Cursor | Agent::OpenCode => {
            Some(Style::default().fg(p.text).add_modifier(Modifier::BOLD))
        }
        // Brand-colored agents (Claude, Codex, Gemini, Antigravity,
        // GithubCopilot, Devin, Kiro, Amp) already returned above via
        // `agent_brand_color`; every remaining agent has no brand identity.
        Agent::Claude
        | Agent::Codex
        | Agent::Gemini
        | Agent::Antigravity
        | Agent::GithubCopilot
        | Agent::Devin
        | Agent::Kiro
        | Agent::Amp
        | Agent::Pi
        | Agent::Cline
        | Agent::Omp
        | Agent::Mastracode
        | Agent::Kimi
        | Agent::Droid
        | Agent::Hermes
        | Agent::Kilo
        | Agent::Qodercli
        | Agent::Qwen
        | Agent::Maki
        | Agent::Muse => None,
    }
}

fn apply_token_style(mut style: Style, patch: crate::config::SidebarTokenStyle) -> Style {
    if let Some(foreground) = patch.fg {
        style = style.fg(foreground.ratatui());
    }
    if let Some(bold) = patch.bold {
        style = if bold {
            style.add_modifier(Modifier::BOLD)
        } else {
            style.remove_modifier(Modifier::BOLD)
        };
    }
    if let Some(dim) = patch.dim {
        style = if dim {
            style.add_modifier(Modifier::DIM)
        } else {
            style.remove_modifier(Modifier::DIM)
        };
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SidebarTokenStyle;
    use crate::detect::Agent;
    use ratatui::style::Color;

    fn row(kinds: Vec<ResolvedTokenKind>) -> Vec<ResolvedToken> {
        kinds.into_iter().map(ResolvedToken::unstyled).collect()
    }

    fn spans_for(
        resolved: &[ResolvedToken],
        agent: Option<Agent>,
        secondary_style: Style,
    ) -> Vec<Span<'static>> {
        spans_for_with_logo(resolved, agent, secondary_style, false).spans
    }

    fn spans_for_with_logo(
        resolved: &[ResolvedToken],
        agent: Option<Agent>,
        secondary_style: Style,
        logo_will_render: bool,
    ) -> ResolvedTokenLine {
        resolved_token_spans(
            resolved,
            ("*", Style::default()),
            Style::default(),
            Style::default(),
            secondary_style,
            Style::default(),
            &Palette::catppuccin(),
            80,
            agent,
            logo_will_render,
        )
    }

    #[test]
    fn branded_agent_colors_the_agent_name_token() {
        let spans = spans_for(
            &row(vec![ResolvedTokenKind::Agent("claude".into())]),
            Some(Agent::Claude),
            Style::default().fg(Color::Magenta),
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(
            spans[0].style.fg,
            agent_logo::agent_brand_color(Agent::Claude)
        );
    }

    #[test]
    fn bold_only_agent_uses_theme_text_without_a_brand_color() {
        let palette = Palette::catppuccin();
        let spans = spans_for(
            &row(vec![ResolvedTokenKind::Agent("grok".into())]),
            Some(Agent::Grok),
            Style::default().fg(Color::Magenta),
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.fg, Some(palette.text));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn unbranded_agent_falls_through_to_secondary_style() {
        let secondary = Style::default().fg(Color::Magenta);
        let spans = spans_for(
            &row(vec![ResolvedTokenKind::Agent("pi".into())]),
            Some(Agent::Pi),
            secondary,
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, secondary);
    }

    #[test]
    fn agent_token_with_no_canonical_agent_falls_through_to_secondary_style() {
        let secondary = Style::default().fg(Color::Magenta);
        let spans = spans_for(
            &row(vec![ResolvedTokenKind::Agent("unknown".into())]),
            None,
            secondary,
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, secondary);
    }

    #[test]
    fn tab_token_renders_teal() {
        let palette = Palette::catppuccin();
        let spans = spans_for(
            &row(vec![ResolvedTokenKind::Tab("main".into())]),
            None,
            Style::default(),
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.fg, Some(palette.teal));
    }

    #[test]
    fn agent_icon_falls_back_to_brand_colored_name_with_no_blank_gap() {
        let spans = spans_for(
            &row(vec![
                ResolvedTokenKind::AgentIcon {
                    fallback: "claude".into(),
                },
                ResolvedTokenKind::Pane("main".into()),
            ]),
            Some(Agent::Claude),
            Style::default(),
        );
        // The icon must render as real, non-empty text (the brand-colored
        // name) rather than an empty cell that leaves a gap before the
        // separator and next token.
        assert_eq!(spans.len(), 3, "expected icon, separator, pane spans");
        assert_eq!(spans[0].content, "claude");
        assert!(!spans[0].content.is_empty());
        assert_eq!(
            spans[0].style.fg,
            agent_logo::agent_brand_color(Agent::Claude)
        );
    }

    #[test]
    fn agent_icon_elides_instead_of_duplicating_a_row_that_already_names_the_agent() {
        let spans = spans_for(
            &row(vec![
                ResolvedTokenKind::Agent("claude".into()),
                ResolvedTokenKind::AgentIcon {
                    fallback: "claude".into(),
                },
            ]),
            Some(Agent::Claude),
            Style::default(),
        );
        // The row already prints the agent name via its own `agent` token;
        // the icon token contributes nothing so the name is not doubled.
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "claude");
    }

    #[test]
    fn agent_icon_falls_back_for_an_agent_with_no_bundled_mask() {
        // Devin has a brand color but ships no logo mask, so its icon token
        // must still degrade cleanly to the brand-colored name.
        assert!(agent_logo::agent_logo_mask(Agent::Devin).is_none());
        let spans = spans_for(
            &row(vec![ResolvedTokenKind::AgentIcon {
                fallback: "devin".into(),
            }]),
            Some(Agent::Devin),
            Style::default(),
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "devin");
        assert_eq!(
            spans[0].style.fg,
            agent_logo::agent_brand_color(Agent::Devin)
        );
    }

    #[test]
    fn same_agent_across_independent_panes_resolves_the_same_brand_style() {
        // Two sidebar rows for two different panes running the same agent
        // must each derive an identical brand style independently — the
        // style (and, once wired, the shared logo image id) is keyed purely
        // by agent identity, never by which pane or row is asking.
        let pane_one = spans_for(
            &row(vec![ResolvedTokenKind::AgentIcon {
                fallback: "claude".into(),
            }]),
            Some(Agent::Claude),
            Style::default(),
        );
        let pane_two = spans_for(
            &row(vec![ResolvedTokenKind::AgentIcon {
                fallback: "claude".into(),
            }]),
            Some(Agent::Claude),
            Style::default(),
        );
        assert_eq!(pane_one[0].style, pane_two[0].style);
        assert_eq!(
            agent_logo::logo_image_id(Agent::Claude),
            agent_logo::logo_image_id(Agent::Claude)
        );
    }

    #[test]
    fn token_level_style_override_still_applies_on_top_of_brand_color() {
        let mut resolved = row(vec![ResolvedTokenKind::Agent("claude".into())]);
        resolved[0].style = SidebarTokenStyle {
            fg: None,
            bold: Some(true),
            dim: None,
        };
        let spans = spans_for(&resolved, Some(Agent::Claude), Style::default());
        assert_eq!(spans.len(), 1);
        assert_eq!(
            spans[0].style.fg,
            agent_logo::agent_brand_color(Agent::Claude)
        );
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn real_logo_reserves_a_fixed_width_box_and_reports_its_column() {
        let line = spans_for_with_logo(
            &row(vec![
                ResolvedTokenKind::AgentIcon {
                    fallback: "claude".into(),
                },
                ResolvedTokenKind::Pane("main".into()),
            ]),
            Some(Agent::Claude),
            Style::default(),
            true,
        );
        assert_eq!(line.spans.len(), 3, "expected icon box, separator, pane");
        assert_eq!(
            display_width(&line.spans[0].content),
            agent_logo::LOGO_COLS as usize
        );
        assert_eq!(line.logo_column, Some(0));
    }

    #[test]
    fn real_logo_never_elides_even_when_the_row_also_names_the_agent() {
        // Text elision exists only to avoid printing the agent name twice;
        // a real logo image is not text, so it must always render alongside
        // an `agent` token instead of disappearing like the text fallback
        // does in `agent_icon_elides_instead_of_duplicating_a_row_that_already_names_the_agent`.
        let line = spans_for_with_logo(
            &row(vec![
                ResolvedTokenKind::Agent("claude".into()),
                ResolvedTokenKind::AgentIcon {
                    fallback: "claude".into(),
                },
            ]),
            Some(Agent::Claude),
            Style::default(),
            true,
        );
        assert_eq!(
            line.spans.len(),
            3,
            "expected agent name, separator, logo box"
        );
        assert!(line.logo_column.is_some());
    }

    #[test]
    fn logo_column_accounts_for_preceding_token_width() {
        let line = spans_for_with_logo(
            &row(vec![
                ResolvedTokenKind::StateIcon,
                ResolvedTokenKind::AgentIcon {
                    fallback: "claude".into(),
                },
            ]),
            Some(Agent::Claude),
            Style::default(),
            true,
        );
        // "*" (state icon) + 1 separator column precede the logo box.
        let expected = display_width("*") as u16 + display_width(" ") as u16;
        assert_eq!(line.logo_column, Some(expected));
    }
}
