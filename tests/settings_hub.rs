use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use moviebox_tui::{
    player::PlayerKind,
    tui::{
        action::Action,
        app::App,
        state::{AppState, SettingsCategory},
        widgets::settings::{settings_category_tab_at, settings_row_at, settings_row_rects},
    },
};
use ratatui::layout::Rect;

#[test]
fn test_settings_toggle_modes_safety_guard() {
    let mut state = AppState {
        streaming_enabled: true,
        tv_enabled: false,
        addons_enabled: false,
        ..Default::default()
    };
    assert!(!state.can_disable_streaming_mode());
    assert!(state.can_disable_tv_mode());

    state.tv_enabled = true;
    assert!(state.can_disable_streaming_mode());
    assert!(state.can_disable_tv_mode());
}

#[tokio::test]
async fn test_settings_keyboard_navigation_and_actions() {
    let mut app = App::new();

    app.handle_action(Action::ToggleSettingsPopup).await;
    assert!(app.state().show_settings_popup);
    assert_eq!(app.state().settings_category, SettingsCategory::General);
    assert_eq!(app.state().settings_selected_row, 0);

    app.handle_action(Action::Key(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::empty(),
    )))
    .await;
    assert_eq!(
        app.state().settings_category,
        SettingsCategory::ContentModes
    );

    app.handle_action(Action::Key(KeyEvent::new(
        KeyCode::BackTab,
        KeyModifiers::empty(),
    )))
    .await;
    assert_eq!(app.state().settings_category, SettingsCategory::General);

    app.handle_action(Action::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::empty(),
    )))
    .await;
    assert_eq!(app.state().settings_selected_row, 1);

    app.state_mut().available_players = vec![PlayerKind::Mpv, PlayerKind::Vlc];
    app.state_mut().default_player = None;
    app.handle_action(Action::SettingsAdjustValue(true)).await;
    assert_eq!(app.state().default_player, Some("vlc".to_string()));

    app.handle_action(Action::SettingsActivateRow).await;
    assert!(app.state().player_picker_popup);
    assert!(app.state().settings_player_picker);

    app.handle_action(Action::MoveDown).await;
    assert_eq!(app.state().player_picker_state.selected(), Some(0));

    app.handle_action(Action::Submit).await;
    assert!(!app.state().player_picker_popup);
    assert!(!app.state().settings_player_picker);
    assert!(app.state().show_settings_popup);
    assert_eq!(app.state().default_player, Some("mpv".to_string()));
    app.handle_action(Action::Key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::empty(),
    )))
    .await;
    assert!(!app.state().show_settings_popup);
}

#[tokio::test]
async fn test_settings_modes_toggle_keeps_popup_open() {
    let mut app = App::new();
    app.state_mut().streaming_enabled = true;
    app.state_mut().tv_enabled = true;
    app.state_mut().addons_enabled = true;
    app.state_mut()
        .set_mode(moviebox_tui::tui::state::AppMode::Streaming);

    app.handle_action(Action::ToggleSettingsPopup).await;
    assert!(app.state().show_settings_popup);

    app.handle_action(Action::SelectSettingsCategory(
        SettingsCategory::ContentModes,
    ))
    .await;
    assert!(app.state().show_settings_popup);
    assert_eq!(
        app.state().settings_category,
        SettingsCategory::ContentModes
    );

    app.state_mut().settings_selected_row = 1;
    app.handle_action(Action::SettingsActivateRow).await;
    assert!(app.state().show_settings_popup);
    assert!(app.state().show_sources_popup);
    app.state_mut().show_sources_popup = false;

    app.state_mut().settings_selected_row = 2;
    app.state_mut().tv_enabled = false;
    app.handle_action(Action::SettingsActivateRow).await;
    assert!(app.state().show_settings_popup);
    assert!(app.state().tv_enabled);
    app.state_mut().settings_selected_row = 0;
    app.state_mut().streaming_enabled = true;
    app.handle_action(Action::SettingsActivateRow).await;
    assert!(app.state().show_settings_popup);
    assert!(!app.state().streaming_enabled);
    assert!(app.state().is_tv_mode);

    app.handle_action(Action::SettingsActivateRow).await;
    assert!(app.state().show_settings_popup);

    app.state_mut()
        .set_mode(moviebox_tui::tui::state::AppMode::Streaming);
    app.state_mut().streaming_enabled = true;
    app.state_mut().tv_enabled = true;
    app.state_mut().addons_enabled = true;
    app.handle_action(Action::ToggleSettingsPopup).await;
    assert!(!app.state().show_settings_popup);
}

#[tokio::test]
async fn test_settings_appearance_theme_cycle_and_popup() {
    let mut app = App::new();
    app.handle_action(Action::ToggleSettingsPopup).await;
    assert!(app.state().show_settings_popup);

    app.handle_action(Action::SelectSettingsCategory(SettingsCategory::Appearance))
        .await;
    assert!(app.state().show_settings_popup);
    assert_eq!(app.state().settings_category, SettingsCategory::Appearance);

    let initial_theme = app.state().active_theme_kind.clone();
    app.handle_action(Action::SettingsAdjustValue(true)).await;
    assert!(app.state().show_settings_popup);
    assert!(!app.state().show_theme_popup);
    assert_ne!(app.state().active_theme_kind, initial_theme);

    app.handle_action(Action::SettingsActivateRow).await;
    assert!(app.state().show_settings_popup);
    assert!(app.state().show_theme_popup);

    app.handle_action(Action::Key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::empty(),
    )))
    .await;
    assert!(app.state().show_settings_popup);
    assert!(!app.state().show_theme_popup);
    app.handle_action(Action::ToggleSettingsPopup).await;
    assert!(!app.state().show_settings_popup);
}

#[tokio::test]
async fn test_settings_mouse_interaction() {
    let mut app = App::new();
    app.handle_action(Action::ToggleSettingsPopup).await;
    assert!(app.state().show_settings_popup);

    app.handle_action(Action::MouseClick(0, 0)).await;
    assert!(!app.state().show_settings_popup);
}

#[tokio::test]
async fn test_settings_mouse_tab_and_row_clicks() {
    let mut app = App::new();
    app.handle_action(Action::ToggleSettingsPopup).await;
    assert!(app.state().show_settings_popup);
    assert_eq!(app.state().settings_category, SettingsCategory::General);

    let popup = Rect::new(4, 4, 76, 17);
    let cat = settings_category_tab_at(popup, 7, popup.y + 1, false, SettingsCategory::General);
    assert_eq!(cat, Some(SettingsCategory::General));

    let cat_modes =
        settings_category_tab_at(popup, 19, popup.y + 1, false, SettingsCategory::General);
    assert_eq!(cat_modes, Some(SettingsCategory::ContentModes));

    let cat_appearance =
        settings_category_tab_at(popup, 37, popup.y + 1, false, SettingsCategory::General);
    assert_eq!(cat_appearance, Some(SettingsCategory::Appearance));

    let cat_maint =
        settings_category_tab_at(popup, 52, popup.y + 1, false, SettingsCategory::General);
    assert_eq!(cat_maint, Some(SettingsCategory::StorageInfo));

    let (_, rows) = settings_row_rects(popup, SettingsCategory::General, 0, 0);
    assert_eq!(rows.len(), 4);
    assert_eq!(
        settings_row_at(
            popup,
            SettingsCategory::General,
            0,
            0,
            rows[0].x + 2,
            rows[0].y
        ),
        Some(0)
    );
    assert_eq!(
        settings_row_at(
            popup,
            SettingsCategory::General,
            0,
            0,
            rows[0].x + 2,
            rows[0].y + 1
        ),
        Some(1)
    );
    assert_eq!(
        settings_row_at(
            popup,
            SettingsCategory::General,
            0,
            0,
            rows[1].x + 2,
            rows[1].y
        ),
        Some(1)
    );
    assert_eq!(
        settings_row_at(
            popup,
            SettingsCategory::General,
            0,
            0,
            rows[2].x + 2,
            rows[2].y
        ),
        Some(2)
    );
    assert_eq!(
        settings_row_at(
            popup,
            SettingsCategory::General,
            0,
            0,
            rows[3].x + 2,
            rows[3].y
        ),
        Some(3)
    );
}

#[test]
fn test_expand_download_path() {
    assert_eq!(AppState::expand_download_path(""), None);
    assert_eq!(AppState::expand_download_path("default"), None);
    assert_eq!(AppState::expand_download_path("reset"), None);

    let path = AppState::expand_download_path("/tmp/moviebox_downloads");
    assert_eq!(
        path,
        Some(std::path::PathBuf::from("/tmp/moviebox_downloads"))
    );
}

#[tokio::test]
async fn test_settings_hub_clear_cache_activation_and_notification() {
    let mut app = App::new();
    app.handle_action(Action::ToggleSettingsPopup).await;
    assert!(app.state().show_settings_popup);

    app.handle_action(Action::SelectSettingsCategory(
        SettingsCategory::StorageInfo,
    ))
    .await;
    assert_eq!(app.state().settings_category, SettingsCategory::StorageInfo);
    app.state_mut().settings_selected_row = 2;
    app.handle_action(Action::SettingsActivateRow).await;
    let clearing = app.state().notifications.back();
    assert_eq!(clearing.map(|n| n.title.as_str()), Some("Clearing Cache"));

    app.handle_action(Action::ClearCache).await;
    assert!(!app.state().show_settings_popup);
    assert_eq!(
        app.state().active_screen,
        moviebox_tui::tui::state::Screen::Home
    );

    app.handle_action(Action::CacheCleared(Ok(()))).await;
    let notification = app.state().notifications.back();
    assert!(notification.is_some());
    assert_eq!(notification.unwrap().title, "Cache Cleared");

    app.handle_action(Action::CacheCleared(Err("filesystem error".to_string())))
        .await;
    let err_notification = app.state().notifications.back();
    assert!(err_notification.is_some());
    assert_eq!(err_notification.unwrap().title, "Cache Clear Failed");
}

#[tokio::test]
async fn test_settings_hub_browser_open_row_activation() {
    let mut app = App::new();
    app.handle_action(Action::ToggleSettingsPopup).await;
    app.handle_action(Action::SelectSettingsCategory(
        SettingsCategory::StorageInfo,
    ))
    .await;
    app.state_mut().settings_selected_row = 4;
    app.handle_action(Action::SettingsActivateRow).await;
    let notification = app.state().notifications.back();
    assert!(notification.is_some());
    let title = &notification.unwrap().title;
    assert!(title == "GitHub" || title == "Browser Launch Failed");
}

#[tokio::test]
async fn test_settings_hub_clear_watch_history_activation() {
    let mut app = App::new();
    let item = moviebox_tui::history::WatchHistoryItem {
        provider: "moviebox".to_string(),
        subject_id: "test-subj-1".to_string(),
        title: "Test Movie".to_string(),
        cover_url: None,
        stype: 1,
        release_year: "2024".to_string(),
        season: 0,
        episode: 0,
        progress_seconds: 120,
        duration_seconds: Some(600),
        completed: false,
        timestamp: 1000,
        stream_filename: None,
    };
    app.state_mut().history.record_start(&item, 120);
    assert!(!app.state().history.recent.is_empty());

    app.handle_action(Action::ToggleSettingsPopup).await;
    app.handle_action(Action::SelectSettingsCategory(
        SettingsCategory::StorageInfo,
    ))
    .await;
    app.state_mut().settings_selected_row = 3;
    app.handle_action(Action::SettingsActivateRow).await;

    assert!(app.state().history.recent.is_empty());
    let notification = app
        .state()
        .notifications
        .back()
        .expect("notification emitted");
    assert_eq!(notification.title, "History");
    assert_eq!(notification.message, "Watch history cleared");
}

#[tokio::test]
async fn test_settings_toonworld_cookie_is_saved_shown_cancelled_and_cleared() {
    use moviebox_tui::providers::toonworld4all::cookie;

    let mut app = App::new();
    app.handle_action(Action::ToggleSettingsPopup).await;
    for _ in 0..3 {
        app.handle_action(Action::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::empty(),
        )))
        .await;
    }
    assert_eq!(app.state().settings_selected_row, 3);

    // Esc abandons an edit without saving anything.
    app.handle_action(Action::SettingsActivateRow).await;
    assert!(app.state().settings_text_input.is_some());
    for c in "user=ignored".chars() {
        app.handle_action(Action::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::empty(),
        )))
        .await;
    }
    app.handle_action(Action::Key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::empty(),
    )))
    .await;
    assert!(app.state().settings_text_input.is_none());
    assert!(cookie::current().is_none());

    // A pasted "Cookie:" header line is saved without its name, and takes effect at once.
    app.handle_action(Action::SettingsActivateRow).await;
    for c in "Cookie: user=abc; x=1".chars() {
        app.handle_action(Action::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::empty(),
        )))
        .await;
    }
    app.handle_action(Action::SettingsActivateRow).await;
    assert!(app.state().settings_text_input.is_none());
    let saved = cookie::current().expect("cookie saved");
    assert_eq!(saved.value, "user=abc; x=1");
    assert!(!saved.is_stale(cookie::now_secs()));

    // Reopening shows the saved value for editing; an empty field removes it.
    app.handle_action(Action::SettingsActivateRow).await;
    assert_eq!(
        app.state().settings_text_input.as_ref().map(|i| i.as_str()),
        Some("user=abc; x=1")
    );
    app.state_mut().settings_text_input = Some(moviebox_tui::tui::text::TextInputBuffer::new());
    app.handle_action(Action::SettingsActivateRow).await;
    assert!(cookie::current().is_none());

    // `d` on the row clears a saved cookie too.
    cookie::save(Some("user=again")).unwrap();
    app.handle_action(Action::Key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::empty(),
    )))
    .await;
    assert!(cookie::current().is_none());
}
