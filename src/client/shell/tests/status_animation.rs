use super::*;
use crate::config::StatusIndicatorStyle;
use crate::protocol::ClientShellAgent;
use std::time::{Duration, Instant};

fn agent_with_status(status: AgentStatus) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: "pane_1".into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: None,
        display_agent: None,
        agent: None,
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 0,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
    }
}

fn state_with_style_and_agent(
    style: StatusIndicatorStyle,
    status: AgentStatus,
) -> ClientShellState {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.status_indicators = style;
    let mut state = ClientShellState::new(config);
    let mut snap = snapshot();
    snap.agents = vec![agent_with_status(status)];
    state.set_snapshot(Box::new(snap));
    state
}

#[test]
fn every_animated_non_working_state_matches_symbols() {
    use crate::client::shell::{status_icon, status_icon_at_frame};
    for status in [
        AgentStatus::Blocked,
        AgentStatus::Done,
        AgentStatus::Idle,
        AgentStatus::Unknown,
    ] {
        assert_eq!(
            status_icon_at_frame(status, StatusIndicatorStyle::Animated, 3),
            status_icon(status, StatusIndicatorStyle::Symbols),
            "Animated must match Symbols for non-working state {status:?}"
        );
    }
}

#[test]
fn working_glyph_advances_and_wraps_at_ten() {
    use crate::client::shell::{status_icon_at_frame, WORKING_SPINNER_FRAMES};
    for frame in 0u8..24 {
        assert_eq!(
            status_icon_at_frame(AgentStatus::Working, StatusIndicatorStyle::Animated, frame),
            WORKING_SPINNER_FRAMES[usize::from(frame) % WORKING_SPINNER_FRAMES.len()],
        );
    }
    // Explicit wrap check: frame 10 must equal frame 0.
    assert_eq!(
        status_icon_at_frame(AgentStatus::Working, StatusIndicatorStyle::Animated, 10),
        status_icon_at_frame(AgentStatus::Working, StatusIndicatorStyle::Animated, 0),
    );
}

#[test]
fn all_spinner_frames_are_single_cell_width() {
    use crate::client::shell::WORKING_SPINNER_FRAMES;
    for frame in WORKING_SPINNER_FRAMES {
        assert_eq!(
            unicode_width::UnicodeWidthStr::width(frame),
            1,
            "spinner frame {frame:?} must be single-cell so the row layout never shifts"
        );
    }
}

// The two guard tests below deliberately tick well past the 100ms cadence.
// Ticking once on fresh state proves nothing: the first call only arms the
// deadline, so it returns false whether or not the idle guard exists. Only a
// tick after the deadline would have elapsed can tell the two apart.
#[test]
fn tick_returns_false_when_style_is_not_animated() {
    let mut state = state_with_style_and_agent(StatusIndicatorStyle::Symbols, AgentStatus::Working);
    let start = Instant::now();
    assert!(!state.tick_status_animation(start));
    assert!(!state.tick_status_animation(start + Duration::from_millis(500)));
    assert_eq!(state.config.status_animation_frame, 0);
}

#[test]
fn tick_returns_false_when_nothing_is_working() {
    let mut state = state_with_style_and_agent(StatusIndicatorStyle::Animated, AgentStatus::Idle);
    let start = Instant::now();
    assert!(!state.tick_status_animation(start));
    assert!(!state.tick_status_animation(start + Duration::from_millis(500)));
    assert_eq!(state.config.status_animation_frame, 0);
}

#[test]
fn tick_advances_frame_only_once_per_hundred_millis_while_working() {
    let mut state =
        state_with_style_and_agent(StatusIndicatorStyle::Animated, AgentStatus::Working);
    let start = Instant::now();

    // First tick only arms the deadline; it must not repaint immediately.
    assert!(!state.tick_status_animation(start));
    assert_eq!(state.config.status_animation_frame, 0);

    // Before the deadline, no advance.
    assert!(!state.tick_status_animation(start + Duration::from_millis(50)));
    assert_eq!(state.config.status_animation_frame, 0);

    // At/after the deadline, the frame advances and the tick reports a repaint.
    assert!(state.tick_status_animation(start + Duration::from_millis(100)));
    assert_eq!(state.config.status_animation_frame, 1);
}
