use super::App;
use crate::tui::action::Action;

impl App {
    pub(super) fn reset_transient_overlays(&mut self) {
        self.state.show_help = false;
        self.state.show_theme_popup = false;
        self.state.show_browse_popup = false;
        self.state.show_provider_popup = false;
        self.state.provider_list_state.select(None);
        self.state.show_settings_popup = false;
        self.state.settings_text_input = None;
        self.state.player_picker_popup = false;
        self.state.settings_player_picker = false;

        self.state.subtitle_popup = false;
        self.state.is_download_subtitle_popup = false;
        self.state.pending_play_link = None;
        self.state.subtitle_list.clear();
        self.state.subtitle_list_state.select(None);
        self.state.is_resolving_playback = false;
        self.state.tv_input_active = false;
        self.state.tv_input_buffer.clear();
        self.state.tv_input_is_file = false;
        self.state.show_overview_modal = false;
        self.state.overview_modal_scroll = 0;
    }

    pub(super) fn reset_mode_state(&mut self) {
        self.state
            .fetch_cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.state.fetch_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.state.provider_generation = self.state.provider_generation.wrapping_add(1);
        self.state.active_preview_request = self.state.active_preview_request.wrapping_add(1);
        self.state.active_search_request = self.state.active_search_request.wrapping_add(1);
        self.state.active_details_request = self.state.active_details_request.wrapping_add(1);
        self.state.active_resource_request = self.state.active_resource_request.wrapping_add(1);
        self.state.tick_count = 0;
        self.reset_transient_overlays();
        self.state.input_mode = crate::tui::state::InputMode::Normal;
        self.state.active_screen = crate::tui::state::Screen::Home;
        self.state.is_loading = false;
        self.state.clear_search_state();
        self.state.clear_details_state();
    }

    pub(super) fn announce_mode(&mut self) {
        match self.state.mode() {
            crate::tui::state::AppMode::Streaming => {
                self.state.set_status_default(format!(
                    "Streaming mode active ({}).",
                    self.state.active_provider.label()
                ));
            }
            crate::tui::state::AppMode::Tv => {
                self.state.set_status_long("Loading TV playlists...");
            }
        }
    }

    pub(super) async fn handle_tv(&mut self, action: Action) -> Option<()> {
        match action {
            Action::SwitchToTvMode => {
                if !self.state.is_tv_mode {
                    self.reset_mode_state();
                    self.state.set_mode(crate::tui::state::AppMode::Tv);
                    self.state.tv_config_popup = false;
                    self.state.tv_channels.clear();
                    self.announce_mode();
                    self.load_tv_playlists_from_config();
                    self.reload_tv_playlists();
                    if self.state.tv_playlists.is_empty() {
                        self.action_sender.send(Action::ShowTvConfig).ok();
                    }
                    self.persist_config();
                }
            }

            Action::ShowTvConfig => {
                if self.state.is_tv_mode {
                    self.reset_transient_overlays();
                    self.state.tv_config_popup = true;
                    self.state.input_mode = crate::tui::state::InputMode::Normal;
                    self.state.tv_manager_selected = 1;
                    self.state.tv_input_active = false;
                    self.state.tv_input_buffer.clear();
                }
            }

            Action::TvPlaylistAdd(source) => {
                let source = source.trim().to_string();
                if !source.is_empty()
                    && !self
                        .state
                        .tv_playlists
                        .iter()
                        .any(|existing| existing == &source)
                {
                    self.state.tv_playlists.push(source);
                    self.save_tv_playlists();
                    self.reload_tv_playlists();
                }
            }

            Action::TvPlaylistRemove(index) => {
                if index < self.state.tv_playlists.len() {
                    self.state.tv_playlists.remove(index);
                    if self.state.tv_manager_selected > self.state.tv_playlists.len() {
                        self.state.tv_manager_selected = self.state.tv_playlists.len();
                    }
                    self.save_tv_playlists();
                    self.reload_tv_playlists();
                }
            }

            Action::TvReloadPlaylists => {
                self.state.set_status_default("Reloading TV playlists...");
                self.reload_tv_playlists();
            }

            Action::TvInputToggle(is_file) => {
                self.state.tv_input_active = true;
                self.state.tv_input_is_file = is_file;
                self.state.tv_input_buffer.clear();
            }

            Action::TvChannelsLoaded(channels, failed) => {
                let mut seen = std::collections::HashSet::new();
                self.state.tv_channels = channels
                    .into_iter()
                    .filter(|channel| {
                        !channel.stream_url.is_empty() && seen.insert(channel.stream_url.clone())
                    })
                    .collect();
                self.state.is_loading = false;
                let query = self.state.search_query.trim().to_string();
                let lower_query = query.to_lowercase();
                if !query.is_empty() {
                    self.apply_tv_search_results(&query, &lower_query);
                    return None;
                }
                self.state
                    .search_list_state
                    .select(if self.state.tv_channels.is_empty() {
                        None
                    } else {
                        Some(0)
                    });
                if self.state.tv_channels.is_empty() {
                    let status = if failed > 0 {
                        format!("No channels. {failed} failed. Add playlist (/config).")
                    } else {
                        "No channels. Add playlist (/config).".to_string()
                    };
                    self.state.set_status_long(status);
                } else {
                    let count = self.state.tv_channels.len();
                    let mut status = format!("{count} channels loaded.");
                    if failed > 0 {
                        status.push_str(&format!(" {failed} failed."));
                    }
                    self.state.set_status_long(status);
                }
            }
            _ => return None,
        }
        None
    }
}
