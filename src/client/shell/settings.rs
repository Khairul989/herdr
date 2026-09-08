use super::*;
use crossterm::event::{KeyCode, KeyModifiers};

pub(super) fn normalized_theme_name(name: &str) -> String {
    name.to_lowercase().replace([' ', '_'], "-")
}

/// The theme picker's full list (curated + generated), narrowed by `filter`
/// using the same case-insensitive multi-word substring match as the help
/// overlay's search. An empty filter matches everything.
pub(super) fn filtered_theme_names(filter: &str) -> Vec<&'static str> {
    crate::app::state::all_theme_names()
        .into_iter()
        .filter(|name| crate::app::state::text_matches_query(filter, name))
        .collect()
}

fn theme_index_in(names: &[&str], name: &str) -> usize {
    let normalized = normalized_theme_name(name);
    names
        .iter()
        .position(|candidate| normalized_theme_name(candidate) == normalized)
        .unwrap_or(0)
}

fn indicator_index(style: crate::config::StatusIndicatorStyle) -> usize {
    match style {
        crate::config::StatusIndicatorStyle::Dots => 0,
        crate::config::StatusIndicatorStyle::Symbols => 1,
        crate::config::StatusIndicatorStyle::Animated => 2,
    }
}

fn toast_index(delivery: crate::config::ToastDelivery) -> usize {
    match delivery {
        crate::config::ToastDelivery::Off => 0,
        crate::config::ToastDelivery::Herdr => 1,
        crate::config::ToastDelivery::Terminal => 2,
        crate::config::ToastDelivery::System => 3,
    }
}

pub(super) fn integration_needs_install(info: &crate::api::schema::IntegrationInfo) -> bool {
    info.state == crate::api::schema::IntegrationState::Outdated
        || info.available && info.state == crate::api::schema::IntegrationState::NotInstalled
}

impl ClientShellState {
    pub(super) fn open_settings_overlay(&mut self) {
        let all_themes = crate::app::state::all_theme_names();
        self.overlay = Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
            section: ClientSettingsSection::Theme,
            selected: theme_index_in(&all_themes, &self.config.theme_name),
            original_theme_name: self.config.theme_name.clone(),
            original_palette: self.config.palette.clone(),
            theme_filter: String::new(),
            integrations: Vec::new(),
            integration_messages: Vec::new(),
            loading_integrations: false,
            installing_integrations: false,
        }));
    }

    /// Selected index for the section being switched TO. For Theme this is
    /// computed against the section's own filter (empty when entering fresh),
    /// so re-selecting Theme always resolves against the filtered list.
    fn selected_index_for_settings_section(&self, section: ClientSettingsSection) -> usize {
        match section {
            ClientSettingsSection::Theme => {
                let filter = match self.overlay.as_ref() {
                    Some(ClientShellOverlay::Settings(settings)) => settings.theme_filter.as_str(),
                    _ => "",
                };
                theme_index_in(&filtered_theme_names(filter), &self.config.theme_name)
            }
            ClientSettingsSection::Indicators => indicator_index(self.config.status_indicators),
            ClientSettingsSection::Sound => usize::from(!self.config.sound_enabled),
            ClientSettingsSection::Toast => toast_index(self.config.toast_delivery),
            ClientSettingsSection::Integrations => 0,
        }
    }

    pub(super) fn select_settings_section(
        &mut self,
        section: ClientSettingsSection,
        outcome: &mut ClientShellInput,
    ) {
        let selected = self.selected_index_for_settings_section(section);
        let request_integrations = matches!(section, ClientSettingsSection::Integrations)
            && matches!(
                self.overlay,
                Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
                    loading_integrations: false,
                    installing_integrations: false,
                    ..
                }))
            );
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            settings.section = section;
            settings.selected = selected;
        }
        if request_integrations {
            self.queue_integration_list(outcome, true);
        }
        outcome.repaint = true;
    }

    fn move_settings_section(&mut self, delta: isize, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_ref() else {
            return;
        };
        let current = ClientSettingsSection::ALL
            .iter()
            .position(|section| *section == settings.section)
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(ClientSettingsSection::ALL.len() as isize)
            as usize;
        self.select_settings_section(ClientSettingsSection::ALL[next], outcome);
    }

    fn settings_choice_count(&self) -> usize {
        match self.overlay.as_ref() {
            Some(ClientShellOverlay::Settings(settings)) => match settings.section {
                ClientSettingsSection::Theme => filtered_theme_names(&settings.theme_filter).len(),
                ClientSettingsSection::Indicators => 3,
                ClientSettingsSection::Sound => 2,
                ClientSettingsSection::Toast => 4,
                ClientSettingsSection::Integrations => settings.integrations.len(),
            },
            _ => 0,
        }
    }

    pub(super) fn move_settings_selection(&mut self, delta: isize) {
        let count = self.settings_choice_count();
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() else {
            return;
        };
        if count == 0 {
            settings.selected = 0;
            return;
        }
        settings.selected = (settings.selected as isize + delta)
            .clamp(0, count.saturating_sub(1) as isize) as usize;
        if settings.section == ClientSettingsSection::Theme {
            self.preview_selected_theme();
        }
    }

    pub(super) fn select_settings_choice(&mut self, index: usize) {
        let count = self.settings_choice_count();
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            if count > 0 {
                settings.selected = index.min(count - 1);
            }
        }
        if matches!(
            self.overlay,
            Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
                section: ClientSettingsSection::Theme,
                ..
            }))
        ) {
            self.preview_selected_theme();
        }
    }

    fn preview_selected_theme(&mut self) {
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_ref() else {
            return;
        };
        let names = filtered_theme_names(&settings.theme_filter);
        let Some(name) = names.get(settings.selected).copied() else {
            return;
        };
        self.config.theme_name = name.to_owned();
        self.config.palette =
            crate::app::client_palette_for_theme(&self.config.theme_runtime, name);
    }

    /// Applies `mutate` to the Theme section's filter, then re-anchors
    /// selection to index 0 and re-previews against the new filtered list —
    /// the filtered list is a subsequence of `all_theme_names()`, so an index
    /// that was valid before a filter edit may point at a different theme
    /// (or nothing) afterward.
    fn mutate_theme_filter(&mut self, mutate: impl FnOnce(&mut String)) {
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            mutate(&mut settings.theme_filter);
            settings.selected = 0;
        }
        self.preview_selected_theme();
    }

    pub(super) fn cancel_settings_overlay(&mut self) {
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.take() else {
            return;
        };
        self.config.theme_name = settings.original_theme_name;
        self.config.palette = settings.original_palette;
    }

    fn save_settings_edit(
        &mut self,
        edit: crate::config::ConfigEdit<'_>,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if let Err(error) = crate::config::write_edit(edit) {
            self.endpoint_error = Some(error);
            outcome.repaint = true;
            return false;
        }
        self.reload_client_config();
        self.push_endpoint_method_with_kind(
            crate::api::schema::Method::ServerReloadConfig(
                crate::api::schema::EmptyParams::default(),
            ),
            PendingEndpointKind::ReloadConfig,
            outcome,
        );
        outcome.repaint = true;
        true
    }

    pub(super) fn apply_settings_choice(&mut self, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_ref() else {
            return;
        };
        let section = settings.section;
        let selected = settings.selected;
        let theme_filter = settings.theme_filter.clone();
        match section {
            ClientSettingsSection::Theme => {
                let names = filtered_theme_names(&theme_filter);
                let Some(name) = names.get(selected).copied() else {
                    return;
                };
                if self.save_settings_edit(crate::config::ConfigEdit::Theme(name), outcome) {
                    self.overlay = None;
                }
            }
            ClientSettingsSection::Indicators => {
                let style = match selected {
                    0 => crate::config::StatusIndicatorStyle::Dots,
                    1 => crate::config::StatusIndicatorStyle::Symbols,
                    _ => crate::config::StatusIndicatorStyle::Animated,
                };
                self.save_settings_edit(
                    crate::config::ConfigEdit::StatusIndicators(style),
                    outcome,
                );
            }
            ClientSettingsSection::Sound => {
                self.save_settings_edit(crate::config::ConfigEdit::Sound(selected == 0), outcome);
            }
            ClientSettingsSection::Toast => {
                let delivery = match selected {
                    0 => crate::config::ToastDelivery::Off,
                    1 => crate::config::ToastDelivery::Herdr,
                    2 => crate::config::ToastDelivery::Terminal,
                    _ => crate::config::ToastDelivery::System,
                };
                self.save_settings_edit(
                    crate::config::ConfigEdit::ToastDelivery(delivery),
                    outcome,
                );
            }
            ClientSettingsSection::Integrations => self.install_recommended_integrations(outcome),
        }
    }

    fn queue_integration_list(&mut self, outcome: &mut ClientShellInput, clear_messages: bool) {
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            settings.loading_integrations = true;
            if clear_messages {
                settings.integration_messages.clear();
            }
        }
        if !self.push_endpoint_method_with_kind(
            crate::api::schema::Method::IntegrationList(crate::api::schema::EmptyParams::default()),
            PendingEndpointKind::IntegrationList,
            outcome,
        ) {
            if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                settings.loading_integrations = false;
            }
        }
    }

    fn install_recommended_integrations(&mut self, outcome: &mut ClientShellInput) {
        if self.pending_integration_installs > 0 {
            return;
        }
        let targets = match self.overlay.as_ref() {
            Some(ClientShellOverlay::Settings(settings)) => settings
                .integrations
                .iter()
                .filter(|integration| integration_needs_install(integration))
                .map(|integration| integration.target)
                .collect::<Vec<_>>(),
            _ => return,
        };
        if targets.is_empty() {
            return;
        }
        if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
            settings.installing_integrations = true;
            settings.integration_messages.clear();
        }
        self.pending_integration_installs = 0;
        for target in targets {
            if self.push_endpoint_method_with_kind(
                crate::api::schema::Method::IntegrationInstall(
                    crate::api::schema::IntegrationInstallParams { target },
                ),
                PendingEndpointKind::IntegrationInstall,
                outcome,
            ) {
                self.pending_integration_installs += 1;
            }
        }
        if self.pending_integration_installs == 0 {
            if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                settings.installing_integrations = false;
            }
        }
        outcome.repaint = true;
    }

    pub(super) fn handle_settings_endpoint_result(
        &mut self,
        kind: PendingEndpointKind,
        result: Result<crate::api::schema::ResponseResult, ClientShellEndpointError>,
    ) -> (bool, Vec<ClientShellAction>) {
        match kind {
            PendingEndpointKind::IntegrationList => {
                if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                    settings.loading_integrations = false;
                    match result {
                        Ok(crate::api::schema::ResponseResult::IntegrationList {
                            integrations,
                        }) => {
                            settings.integrations = integrations;
                            settings.selected = settings
                                .selected
                                .min(settings.integrations.len().saturating_sub(1));
                        }
                        Ok(_) => {
                            self.endpoint_error = Some(
                                "endpoint returned an unexpected integration list result".into(),
                            );
                        }
                        Err(_) => {}
                    }
                }
                (true, Vec::new())
            }
            PendingEndpointKind::IntegrationInstall => {
                let cancelled = result
                    .as_ref()
                    .is_err_and(|error| error.code.as_deref() == Some("endpoint_cancelled"));
                self.pending_integration_installs =
                    self.pending_integration_installs.saturating_sub(1);
                if let Some(ClientShellOverlay::Settings(settings)) = self.overlay.as_mut() {
                    match result {
                        Ok(crate::api::schema::ResponseResult::IntegrationInstall {
                            details,
                            ..
                        }) => settings.integration_messages.extend(details.messages),
                        Ok(_) => settings
                            .integration_messages
                            .push("endpoint returned an unexpected integration result".into()),
                        Err(error) => settings.integration_messages.push(error.message),
                    }
                    settings.installing_integrations = self.pending_integration_installs > 0;
                }
                let actions = if !cancelled
                    && self.pending_integration_installs == 0
                    && matches!(self.overlay, Some(ClientShellOverlay::Settings(_)))
                {
                    let mut deferred = ClientShellInput::default();
                    self.queue_integration_list(&mut deferred, false);
                    deferred.actions
                } else {
                    Vec::new()
                };
                (true, actions)
            }
            _ => (false, Vec::new()),
        }
    }

    pub(super) fn route_settings_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if !matches!(self.overlay, Some(ClientShellOverlay::Settings(_))) {
            return false;
        }
        let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
        let in_theme_section = matches!(
            self.overlay,
            Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
                section: ClientSettingsSection::Theme,
                ..
            }))
        );

        if code == KeyCode::Esc {
            if matches!(
                self.overlay,
                Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
                    installing_integrations: true,
                    ..
                }))
            ) {
                return true;
            }
            let filter_is_open = matches!(
                self.overlay,
                Some(ClientShellOverlay::Settings(ClientSettingsOverlay {
                    section: ClientSettingsSection::Theme,
                    ref theme_filter,
                    ..
                })) if !theme_filter.is_empty()
            );
            if filter_is_open {
                self.mutate_theme_filter(|filter| filter.clear());
            } else {
                self.cancel_settings_overlay();
            }
            outcome.repaint = true;
            return true;
        }
        // In the Theme section, plain h/l/j/k feed the filter text instead of
        // navigating — only the dedicated nav keys below move sections/selection.
        if (matches!(code, KeyCode::Tab | KeyCode::Right)
            || (!in_theme_section && code == KeyCode::Char('l')))
            && modifiers.is_empty()
        {
            self.move_settings_section(1, outcome);
            return true;
        }
        if (matches!(code, KeyCode::BackTab | KeyCode::Left)
            || (!in_theme_section && code == KeyCode::Char('h')))
            && modifiers.difference(KeyModifiers::SHIFT).is_empty()
        {
            self.move_settings_section(-1, outcome);
            return true;
        }
        let move_up = (code == KeyCode::Up && modifiers.is_empty())
            || (in_theme_section
                && code == KeyCode::Char('p')
                && modifiers.contains(KeyModifiers::CONTROL))
            || (!in_theme_section && code == KeyCode::Char('k') && modifiers.is_empty());
        if move_up {
            self.move_settings_selection(-1);
            outcome.repaint = true;
            return true;
        }
        let move_down = (code == KeyCode::Down && modifiers.is_empty())
            || (in_theme_section
                && code == KeyCode::Char('n')
                && modifiers.contains(KeyModifiers::CONTROL))
            || (!in_theme_section && code == KeyCode::Char('j') && modifiers.is_empty());
        if move_down {
            self.move_settings_selection(1);
            outcome.repaint = true;
            return true;
        }
        if matches!(code, KeyCode::Enter | KeyCode::Char(' ')) && modifiers.is_empty() {
            self.apply_settings_choice(outcome);
            return true;
        }
        if in_theme_section {
            if code == KeyCode::Char('u') && modifiers.contains(KeyModifiers::CONTROL) {
                self.mutate_theme_filter(|filter| filter.clear());
                outcome.repaint = true;
                return true;
            }
            if code == KeyCode::Backspace && modifiers.is_empty() {
                self.mutate_theme_filter(|filter| {
                    filter.pop();
                });
                outcome.repaint = true;
                return true;
            }
            if let KeyCode::Char(character) = code {
                if modifiers.difference(KeyModifiers::SHIFT).is_empty() {
                    let generated_text = key.generated_text.clone();
                    self.mutate_theme_filter(|filter| match generated_text.as_deref() {
                        Some(text) => filter.push_str(text),
                        None => filter.push(character),
                    });
                    outcome.repaint = true;
                    return true;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_state() -> ClientShellState {
        ClientShellState::new(ClientShellConfig::from_config(&Config::default()))
    }

    fn theme_filter(state: &ClientShellState) -> &str {
        match state.overlay.as_ref() {
            Some(ClientShellOverlay::Settings(settings)) => settings.theme_filter.as_str(),
            _ => panic!("settings overlay must be open"),
        }
    }

    fn theme_selected(state: &ClientShellState) -> usize {
        match state.overlay.as_ref() {
            Some(ClientShellOverlay::Settings(settings)) => settings.selected,
            _ => panic!("settings overlay must be open"),
        }
    }

    #[test]
    fn indicator_index_is_distinct_for_every_style() {
        // The modal highlights a row by index and saves the style at the
        // selected index. If two styles share an index, choosing one silently
        // saves the other.
        let styles = [
            crate::config::StatusIndicatorStyle::Dots,
            crate::config::StatusIndicatorStyle::Symbols,
            crate::config::StatusIndicatorStyle::Animated,
        ];
        let mut indices: Vec<usize> = styles.iter().map(|s| indicator_index(*s)).collect();
        indices.sort_unstable();
        indices.dedup();
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn theme_filter_predicate_is_multi_word_and_no_match_is_empty() {
        assert!(filtered_theme_names("tokyo night")
            .iter()
            .any(|name| name.eq_ignore_ascii_case("tokyonight-storm")));
        assert!(filtered_theme_names("not-a-real-theme-query").is_empty());
    }

    #[test]
    fn selection_stays_in_range_when_filter_shrinks_the_list() {
        let mut state = test_state();
        state.open_settings_overlay();

        // Narrow to a query with several matches and select the last one.
        let mut outcome = ClientShellInput::default();
        for character in "storm".chars() {
            state.route_settings_key(
                &crate::input::TerminalKey::new(KeyCode::Char(character), KeyModifiers::NONE),
                &mut outcome,
            );
        }
        let storm_count = filtered_theme_names(theme_filter(&state)).len();
        assert!(
            storm_count > 1,
            "expected more than one theme matching 'storm' to exercise a shrinking filter"
        );
        for _ in 0..storm_count {
            state.move_settings_selection(1);
        }
        assert_eq!(theme_selected(&state), storm_count - 1);

        // Narrowing further must re-anchor selection into range rather than
        // leaving it pointing past the end of the smaller filtered list.
        state.route_settings_key(
            &crate::input::TerminalKey::new(KeyCode::Char('z'), KeyModifiers::NONE),
            &mut outcome,
        );
        let narrowed_count = filtered_theme_names(theme_filter(&state)).len();
        assert!(narrowed_count < storm_count);
        assert_eq!(theme_selected(&state), 0);
        assert!(theme_selected(&state) < narrowed_count.max(1));
    }

    #[test]
    fn esc_clears_filter_before_cancelling_overlay() {
        let mut state = test_state();
        state.open_settings_overlay();
        let original_theme = state.config.theme_name.clone();
        let mut outcome = ClientShellInput::default();

        state.route_settings_key(
            &crate::input::TerminalKey::new(KeyCode::Char('x'), KeyModifiers::NONE),
            &mut outcome,
        );
        assert_eq!(theme_filter(&state), "x");

        // First Esc clears the filter but keeps the overlay open.
        state.route_settings_key(
            &crate::input::TerminalKey::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut outcome,
        );
        assert!(matches!(
            state.overlay,
            Some(ClientShellOverlay::Settings(_))
        ));
        assert_eq!(theme_filter(&state), "");

        // Second Esc (filter already empty) cancels the overlay.
        state.route_settings_key(
            &crate::input::TerminalKey::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut outcome,
        );
        assert!(state.overlay.is_none());
        assert_eq!(state.config.theme_name, original_theme);
    }
}
