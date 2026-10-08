use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::{
    overlay,
    state::{AppState, SettingsCategory, settings_player_label},
    theme::Theme,
    widgets::ModalFrame,
};

/// Columns the five full tab titles need with their gaps; narrower tab rows use short titles.
const FULL_TABS_WIDTH: u16 = 66;

pub fn category_tab_rects(
    tabs_area: Rect,
    _basic_terminal: bool,
    _active_cat: SettingsCategory,
) -> Vec<(SettingsCategory, Rect)> {
    let mut results = Vec::new();
    let mut current_x = tabs_area.x;
    let compact = tabs_area.width < FULL_TABS_WIDTH;
    let gap = if compact { 2 } else { 3 };

    for cat in SettingsCategory::ALL {
        let title = if compact {
            cat.compact_title()
        } else {
            cat.title()
        };
        let width = crate::tui::text::width(title) as u16;
        if current_x + width <= tabs_area.right() {
            results.push((
                cat,
                Rect {
                    x: current_x,
                    y: tabs_area.y,
                    width,
                    height: 2,
                },
            ));
        }
        current_x = current_x.saturating_add(width).saturating_add(gap);
    }
    results
}

pub fn settings_category_tab_at(
    popup_area: Rect,
    col: u16,
    row: u16,
    basic_terminal: bool,
    active_cat: SettingsCategory,
) -> Option<SettingsCategory> {
    if popup_area.width < 4 || popup_area.height < 4 {
        return None;
    }
    let inner_y = popup_area.y + 1;
    if row != inner_y && row != inner_y + 1 {
        return None;
    }
    let tabs_area = Rect {
        x: popup_area.x + 3,
        y: inner_y,
        width: popup_area.width.saturating_sub(6),
        height: 2,
    };
    for (cat, rect) in category_tab_rects(tabs_area, basic_terminal, active_cat) {
        if col >= rect.x && col < rect.right() && row >= rect.y && row < rect.bottom() {
            return Some(cat);
        }
    }
    None
}

/// Splits the modal's inner area into tabs, rows and (for Env Variables) the help footer.
fn category_sections(inner: Rect, category: SettingsCategory) -> (Rect, Rect, Rect) {
    let footer = category.footer_height() as u16;
    let constraints = if footer > 0 {
        [
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(footer),
        ]
    } else {
        [
            Constraint::Length(2),
            Constraint::Length(category.row_count() as u16),
            Constraint::Min(0),
        ]
    };
    let sections = Layout::vertical(constraints).split(inner);
    (sections[0], sections[1], sections[2])
}

/// First row shown when only `visible` rows fit: moves `previous` just enough to keep the
/// selected row on screen, so the list doesn't jump.
pub fn settings_scroll(
    category: SettingsCategory,
    previous: usize,
    selected: usize,
    visible: usize,
) -> usize {
    let count = category.row_count();
    if visible == 0 || count <= visible {
        return 0;
    }
    let selected = selected.min(count - 1);
    previous
        .clamp(selected.saturating_sub(visible - 1), selected)
        .min(count - visible)
}

/// Screen rects of the visible rows; `rects[i]` is row `scroll + i`, where `scroll` is
/// [`settings_scroll`] for `selected`.
pub fn settings_row_rects(
    popup_area: Rect,
    category: SettingsCategory,
    previous_scroll: usize,
    selected: usize,
) -> (usize, Vec<Rect>) {
    if popup_area.width < 4 || popup_area.height < 4 {
        return (0, Vec::new());
    }
    let inner = Rect {
        x: popup_area.x + 1,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(2),
        height: popup_area.height.saturating_sub(2),
    };
    let (_, rows_area, _) = category_sections(inner, category);
    let scroll = settings_scroll(
        category,
        previous_scroll,
        selected,
        rows_area.height as usize,
    );
    let card_x = popup_area.x + 2;
    let card_width = popup_area.width.saturating_sub(4);
    let visible = category
        .row_count()
        .saturating_sub(scroll)
        .min(rows_area.height as usize);
    let rects = (0..visible)
        .map(|i| Rect {
            x: card_x,
            y: rows_area.y + i as u16,
            width: card_width,
            height: 1,
        })
        .collect();
    (scroll, rects)
}

pub fn settings_row_at(
    popup_area: Rect,
    category: SettingsCategory,
    previous_scroll: usize,
    selected: usize,
    col: u16,
    row: u16,
) -> Option<usize> {
    let (scroll, rects) = settings_row_rects(popup_area, category, previous_scroll, selected);
    rects
        .into_iter()
        .position(|rect| {
            row >= rect.y && row < rect.bottom() && col >= rect.x && col < rect.right()
        })
        .map(|idx| scroll + idx)
}

pub fn draw(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    let popup_area = overlay::settings_modal_layout(area, state.settings_category);
    let title = " Settings & Preferences ";
    let has_popup = has_active_settings_popup(state);
    let mut modal = ModalFrame::new(title, theme, state.basic_terminal);
    if has_popup {
        modal = modal.border_style(theme.muted).title_style(theme.muted);
    }
    let inner = modal.render(frame, popup_area, area);
    if inner.width < 10 || inner.height < 3 {
        return;
    }

    let (tabs_area, rows_area, footer_area) = category_sections(inner, state.settings_category);
    render_tabs(frame, tabs_area, popup_area, state, theme);
    render_category_rows(frame, rows_area, popup_area, state, theme);
    if state.settings_category == SettingsCategory::EnvVars {
        render_env_footer(frame, footer_area, popup_area, state, theme);
    }
}

fn render_tabs(frame: &mut Frame, area: Rect, popup_area: Rect, state: &AppState, theme: &Theme) {
    let mut line0_spans = Vec::new();
    let mut line1_spans = Vec::new();

    let compact = popup_area.width.saturating_sub(6) < FULL_TABS_WIDTH;
    for (i, cat) in SettingsCategory::ALL.iter().enumerate() {
        if i > 0 {
            let gap = if compact { "  " } else { "   " };
            line0_spans.push(Span::raw(gap));
            line1_spans.push(Span::raw(gap));
        }
        let is_active = *cat == state.settings_category;
        let title = if compact {
            cat.compact_title()
        } else {
            cat.title()
        };
        let width = crate::tui::text::width(title);

        if is_active {
            let (title_style, underline_style) = if has_active_settings_popup(state) {
                (theme.muted, theme.muted)
            } else if state.basic_terminal {
                (theme.text.add_modifier(Modifier::BOLD), theme.text)
            } else {
                (theme.accent.add_modifier(Modifier::BOLD), theme.accent)
            };
            line0_spans.push(Span::styled(title, title_style));
            line1_spans.push(Span::styled("─".repeat(width), underline_style));
        } else {
            let title_style = if has_active_settings_popup(state) {
                theme.muted
            } else if state.basic_terminal {
                theme.text_dim
            } else {
                theme.subtext1
            };
            line0_spans.push(Span::styled(title, title_style));
            line1_spans.push(Span::raw(" ".repeat(width)));
        }
    }

    let tabs_render_area = Rect {
        x: popup_area.x + 3,
        y: area.y,
        width: popup_area.width.saturating_sub(4),
        height: area.height.min(2),
    };
    let lines = if area.height >= 2 {
        vec![Line::from(line0_spans), Line::from(line1_spans)]
    } else {
        vec![Line::from(line0_spans)]
    };
    frame.render_widget(Paragraph::new(lines), tabs_render_area);
}

fn render_category_rows(
    frame: &mut Frame,
    area: Rect,
    popup_area: Rect,
    state: &AppState,
    theme: &Theme,
) {
    let rows_area = Rect {
        x: popup_area.x + 2,
        y: area.y,
        width: popup_area.width.saturating_sub(4),
        height: area.height,
    };
    match state.settings_category {
        SettingsCategory::General => render_general_settings(frame, rows_area, state, theme),
        SettingsCategory::ContentModes => {
            render_content_modes_settings(frame, rows_area, state, theme)
        }
        SettingsCategory::Appearance => render_appearance_settings(frame, rows_area, state, theme),
        SettingsCategory::StorageInfo => render_storage_settings(frame, rows_area, state, theme),
        SettingsCategory::EnvVars => render_env_settings(frame, rows_area, state, theme),
    }
}
fn has_active_settings_popup(state: &AppState) -> bool {
    state.settings_player_picker
        || state.show_theme_popup
        || state.show_sources_popup
        || state.player_picker_popup
        || state.show_browse_popup
        || state.settings_text_input.is_some()
}

struct SettingRow<'a> {
    is_selected: bool,
    has_active_popup: bool,
    label: &'a str,
    value_spans: Vec<Span<'a>>,
}
fn on_off_spans<'a>(
    enabled: bool,
    has_active_popup: bool,
    theme: &Theme,
    basic_terminal: bool,
) -> Vec<Span<'a>> {
    if has_active_popup {
        if enabled {
            vec![Span::styled("ON", theme.muted)]
        } else {
            vec![Span::styled("OFF", theme.muted)]
        }
    } else if enabled {
        vec![Span::styled(
            "ON",
            if basic_terminal {
                theme.text.add_modifier(Modifier::BOLD)
            } else {
                theme.success.add_modifier(Modifier::BOLD)
            },
        )]
    } else {
        vec![Span::styled(
            "OFF",
            if basic_terminal {
                theme.text_dim
            } else {
                theme.overlay1
            },
        )]
    }
}

fn render_row(
    frame: &mut Frame,
    area: Rect,
    row: SettingRow<'_>,
    theme: &Theme,
    basic_terminal: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let is_active_selection = row.is_selected && !row.has_active_popup;
    let label_style = if row.has_active_popup {
        theme.muted
    } else if is_active_selection {
        theme.highlight.add_modifier(Modifier::BOLD)
    } else if row.is_selected {
        theme.text.add_modifier(Modifier::BOLD)
    } else {
        theme.text
    };

    let row_bg = if is_active_selection {
        crate::tui::overlay::selection_style(theme, basic_terminal)
    } else {
        Style::default()
    };
    let label_width = crate::tui::text::width(row.label);

    let right_width: usize = row
        .value_spans
        .iter()
        .map(|s| crate::tui::text::width(&s.content))
        .sum();

    let right_margin = 3;
    let left_margin = 2;
    let area_width = area.width as usize;
    let max_label_w = area_width.saturating_sub(left_margin + right_width + right_margin + 1);
    let display_label = if label_width > max_label_w && max_label_w >= 4 {
        crate::tui::text::truncate_width(row.label, max_label_w)
    } else {
        std::borrow::Cow::Borrowed(row.label)
    };
    let display_label_w = crate::tui::text::width(&display_label);
    let pad = area_width.saturating_sub(left_margin + display_label_w + right_width + right_margin);

    let mut line_spans = Vec::new();
    line_spans.push(Span::raw(" ".repeat(left_margin)));
    line_spans.push(Span::styled(display_label, label_style));
    if pad > 0 {
        line_spans.push(Span::raw(" ".repeat(pad)));
    } else {
        line_spans.push(Span::raw(" "));
    }
    line_spans.extend(row.value_spans);
    line_spans.push(Span::raw(" ".repeat(right_margin)));

    frame.render_widget(Paragraph::new(Line::from(line_spans)).style(row_bg), area);
}

fn render_general_settings(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let row_rects = settings_row_rects_in_area(area, 3);
    let has_active_popup = has_active_settings_popup(state);

    if let Some(&row_area) = row_rects.first() {
        let is_selected = state.settings_selected_row == 0;
        let value_spans = on_off_spans(
            state.auto_update,
            has_active_popup,
            theme,
            state.basic_terminal,
        );
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Automatic Updates",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }
    if let Some(&row_area) = row_rects.get(1) {
        let is_selected = state.settings_selected_row == 1;
        let player_name = if let Some(key) = state
            .default_player
            .as_deref()
            .filter(|k| !k.is_empty() && *k != "auto")
        {
            settings_player_label(Some(key))
        } else if let Some(first) = state.available_players.first() {
            settings_player_label(Some(first.config_key()))
        } else {
            settings_player_label(None)
        };
        let value_spans = vec![Span::styled(
            player_name,
            if has_active_popup {
                theme.muted
            } else if state.basic_terminal {
                theme.text.add_modifier(Modifier::BOLD)
            } else {
                theme.accent.add_modifier(Modifier::BOLD)
            },
        )];
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Default Media Player",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }

    if let Some(&row_area) = row_rects.get(2) {
        let is_selected = state.settings_selected_row == 2;
        let value_spans =
            if let Some(input) = state.settings_text_input.as_ref().filter(|_| is_selected) {
                let cursor_char = if state.basic_terminal { "_" } else { "▌" };
                let input_str = input.as_str();
                let cursor_offset = input.cursor_byte_offset();
                let (before, after) = input_str.split_at(cursor_offset);
                let input_budget = (row_area.width as usize).saturating_sub(21).clamp(10, 60);
                let truncated_before =
                    crate::tui::text::truncate_width(before, input_budget.saturating_sub(4));
                if state.basic_terminal {
                    vec![
                        Span::styled(truncated_before, theme.text),
                        Span::styled(cursor_char, theme.text.add_modifier(Modifier::BOLD)),
                        Span::styled(after, theme.text),
                    ]
                } else {
                    let input_bg = theme.surface0_color();
                    let input_style = Style::default()
                        .fg(theme
                            .text
                            .fg
                            .unwrap_or(theme.subtext1.fg.unwrap_or(theme.base)))
                        .bg(input_bg);
                    let cursor_style = theme.accent.add_modifier(Modifier::BOLD).bg(input_bg);
                    vec![
                        Span::styled(truncated_before, input_style),
                        Span::styled(cursor_char, cursor_style),
                        Span::styled(after, input_style),
                    ]
                }
            } else {
                let path_str = state
                    .download_dir
                    .as_ref()
                    .map(crate::logging::sanitize_path)
                    .unwrap_or_else(|| {
                        crate::logging::sanitize_path(crate::service::resolve_download_dir(None))
                    });
                let path_budget = (row_area.width as usize).saturating_sub(21).clamp(10, 60);
                let truncated = crate::tui::text::truncate_middle_width(&path_str, path_budget);
                vec![Span::styled(
                    truncated,
                    if has_active_popup {
                        theme.muted
                    } else if state.basic_terminal {
                        theme.text_dim
                    } else {
                        theme.subtext1
                    },
                )]
            };
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Download Folder",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }
}

/// An open text field that keeps the cursor end in view, for long pasted values.
fn tail_input_spans<'a>(
    input: &'a crate::tui::text::TextInputBuffer,
    budget: usize,
    state: &AppState,
    theme: &Theme,
) -> Vec<Span<'a>> {
    let cursor_char = if state.basic_terminal { "_" } else { "▌" };
    let (before, after) = input.as_str().split_at(input.cursor_byte_offset());
    let visible_before = crate::tui::text::tail_width(before, budget.saturating_sub(2));
    let after = crate::tui::text::truncate_width(after, 6);
    if state.basic_terminal {
        vec![
            Span::styled(visible_before, theme.text),
            Span::styled(cursor_char, theme.text.add_modifier(Modifier::BOLD)),
            Span::styled(after, theme.text),
        ]
    } else {
        let input_bg = theme.surface0_color();
        let input_style = Style::default()
            .fg(theme
                .text
                .fg
                .unwrap_or(theme.subtext1.fg.unwrap_or(theme.base)))
            .bg(input_bg);
        let cursor_style = theme.accent.add_modifier(Modifier::BOLD).bg(input_bg);
        vec![
            Span::styled(visible_before, input_style),
            Span::styled(cursor_char, cursor_style),
            Span::styled(after, input_style),
        ]
    }
}

fn format_age(secs: u64) -> String {
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

/// The idle text of the ToonWorld cookie row: what is in effect and how old it is.
fn toonworld_cookie_summary<'a>(
    budget: usize,
    base_style: Style,
    has_active_popup: bool,
    theme: &Theme,
) -> Vec<Span<'a>> {
    use crate::providers::toonworld4all::cookie;
    if cookie::from_env().is_some() {
        return vec![Span::styled("from MOVIEBOX_TOONWORLD_COOKIE", base_style)];
    }
    let Some(saved) = cookie::current() else {
        return vec![Span::styled("not set (Enter to paste)", base_style)];
    };
    let age_secs = saved.age_secs(cookie::now_secs());
    let age_text = format!("  {}", format_age(age_secs));
    let value_budget = budget
        .saturating_sub(crate::tui::text::width(&age_text))
        .max(8);
    let age_style = if has_active_popup || !saved.is_stale(cookie::now_secs()) {
        base_style
    } else {
        theme.error
    };
    let age_text = if saved.is_stale(cookie::now_secs()) {
        format!("{age_text}, likely expired")
    } else {
        age_text
    };
    vec![
        Span::styled(
            crate::tui::text::truncate_middle_width(&saved.value, value_budget),
            base_style,
        ),
        Span::styled(age_text, age_style),
    ]
}

fn render_content_modes_settings(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let row_rects = settings_row_rects_in_area(area, 3);
    let has_active_popup = has_active_settings_popup(state);

    if let Some(&row_area) = row_rects.first() {
        let is_selected = state.settings_selected_row == 0;
        let value_spans = on_off_spans(
            state.streaming_enabled,
            has_active_popup,
            theme,
            state.basic_terminal,
        );
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Streaming Mode",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }

    if let Some(&row_area) = row_rects.get(1) {
        let is_selected = state.settings_selected_row == 1;
        let is_active_selected = is_selected && !has_active_popup;
        let enabled_count = crate::providers::models::ProviderKind::ENABLED
            .iter()
            .filter(|p| state.provider_enabled(**p))
            .count();
        let glyph = if state.basic_terminal { ">" } else { "▸" };
        let value_spans = vec![Span::styled(
            format!("{enabled_count} Active {glyph}"),
            if has_active_popup {
                theme.muted
            } else if state.basic_terminal {
                if is_active_selected {
                    theme.text.add_modifier(Modifier::BOLD)
                } else {
                    theme.text_dim
                }
            } else if is_active_selected {
                theme.accent.add_modifier(Modifier::BOLD)
            } else {
                theme.accent
            },
        )];
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Streaming Sources",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }

    if let Some(&row_area) = row_rects.get(2) {
        let is_selected = state.settings_selected_row == 2;
        let value_spans = on_off_spans(
            state.tv_enabled,
            has_active_popup,
            theme,
            state.basic_terminal,
        );
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Live TV Mode",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }
}

fn render_appearance_settings(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let row_rects = settings_row_rects_in_area(area, 1);
    let has_active_popup = has_active_settings_popup(state);

    let theme_name = if state.active_theme_kind.is_empty() {
        "Mocha"
    } else {
        &state.active_theme_kind
    };

    if let Some(&row_area) = row_rects.first() {
        let is_selected = state.settings_selected_row == 0;
        let mut value_spans = vec![Span::styled(
            format!("{theme_name}  "),
            if has_active_popup {
                theme.muted
            } else {
                theme.accent.add_modifier(Modifier::BOLD)
            },
        )];
        if !has_active_popup {
            value_spans.extend(Theme::palette_swatch_spans(
                theme_name,
                state.basic_terminal,
            ));
        }
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Theme",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }
}

fn render_storage_settings(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let row_rects = settings_row_rects_in_area(area, 5);
    let has_active_popup = has_active_settings_popup(state);

    if let Some(&row_area) = row_rects.first() {
        let is_selected = state.settings_selected_row == 0;
        let is_active_selected = is_selected && !has_active_popup;
        let value_spans = if state.is_checking_updates {
            vec![Span::styled(
                "Checking...",
                if has_active_popup {
                    theme.muted
                } else if state.basic_terminal {
                    theme.text_dim
                } else {
                    theme.sapphire
                },
            )]
        } else {
            let glyph = if state.basic_terminal { ">" } else { "▸" };
            vec![Span::styled(
                format!("Check {glyph}"),
                if has_active_popup {
                    theme.muted
                } else if state.basic_terminal {
                    if is_active_selected {
                        theme.text.add_modifier(Modifier::BOLD)
                    } else {
                        theme.text_dim
                    }
                } else if is_active_selected {
                    theme.sapphire.add_modifier(Modifier::BOLD)
                } else {
                    theme.sapphire
                },
            )]
        };
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Check for Updates",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }

    if let Some(&row_area) = row_rects.get(1) {
        let is_selected = state.settings_selected_row == 1;
        let is_active_selected = is_selected && !has_active_popup;
        let glyph = if state.basic_terminal { "->" } else { "▸" };
        let value_spans = vec![Span::styled(
            format!("Run {glyph}"),
            if has_active_popup {
                theme.muted
            } else if state.basic_terminal {
                if is_active_selected {
                    theme.text.add_modifier(Modifier::BOLD)
                } else {
                    theme.text_dim
                }
            } else if is_active_selected {
                theme.sapphire.add_modifier(Modifier::BOLD)
            } else {
                theme.sapphire
            },
        )];
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Re-check BDIX Network",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }

    if let Some(&row_area) = row_rects.get(2) {
        let is_selected = state.settings_selected_row == 2;
        let is_active_selected = is_selected && !has_active_popup;
        let glyph = if state.basic_terminal { ">" } else { "▸" };
        let value_spans = vec![Span::styled(
            format!("Purge {glyph}"),
            if has_active_popup {
                theme.muted
            } else if state.basic_terminal {
                if is_active_selected {
                    theme.text.add_modifier(Modifier::BOLD)
                } else {
                    theme.text_dim
                }
            } else if is_active_selected {
                theme.error.add_modifier(Modifier::BOLD)
            } else {
                theme.error
            },
        )];
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Clear Disk Cache",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }

    if let Some(&row_area) = row_rects.get(3) {
        let is_selected = state.settings_selected_row == 3;
        let is_active_selected = is_selected && !has_active_popup;
        let glyph = if state.basic_terminal { ">" } else { "▸" };
        let value_spans = vec![Span::styled(
            format!("Clear {glyph}"),
            if has_active_popup {
                theme.muted
            } else if state.basic_terminal {
                if is_active_selected {
                    theme.text.add_modifier(Modifier::BOLD)
                } else {
                    theme.text_dim
                }
            } else if is_active_selected {
                theme.error.add_modifier(Modifier::BOLD)
            } else {
                theme.error
            },
        )];
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "Clear Watch History",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }

    if let Some(&row_area) = row_rects.get(4) {
        let is_selected = state.settings_selected_row == 4;
        let is_active_selected = is_selected && !has_active_popup;
        let glyph = if state.basic_terminal { "->" } else { "↗" };
        let value_spans = vec![Span::styled(
            format!("Open {glyph}"),
            if has_active_popup {
                theme.muted
            } else if state.basic_terminal {
                if is_active_selected {
                    theme.text.add_modifier(Modifier::BOLD)
                } else {
                    theme.text_dim
                }
            } else if is_active_selected {
                theme.lavender.add_modifier(Modifier::BOLD)
            } else {
                theme.lavender
            },
        )];
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label: "GitHub Repository",
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }
}

/// Label of an Env Variables row: the name without the shared prefix.
fn env_label(name: &str) -> &str {
    name.strip_prefix("MOVIEBOX_").unwrap_or(name)
}

fn render_env_settings(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    use crate::env_vars::{self, SPECS};
    let has_active_popup = has_active_settings_popup(state);
    let selected = state.settings_selected_row;
    let scroll = settings_scroll(
        SettingsCategory::EnvVars,
        state.settings_scroll.get(),
        selected,
        area.height as usize,
    );
    state.settings_scroll.set(scroll);
    let dim = if has_active_popup {
        theme.muted
    } else if state.basic_terminal {
        theme.text_dim
    } else {
        theme.overlay1
    };
    let rows = settings_row_rects_in_area(area, SPECS.len().saturating_sub(scroll));
    for (row_area, (index, spec)) in rows.into_iter().zip(SPECS.iter().enumerate().skip(scroll)) {
        let is_selected = index == selected;
        let label = env_label(spec.name);
        let budget = (row_area.width as usize)
            .saturating_sub(crate::tui::text::width(label) + 7)
            .clamp(8, 48);
        let value_spans =
            if let Some(input) = state.settings_text_input.as_ref().filter(|_| is_selected) {
                tail_input_spans(input, budget, state, theme)
            } else if spec.kind == env_vars::Kind::Cookie {
                toonworld_cookie_summary(budget, dim, has_active_popup, theme)
            } else if let Some(shell) = env_vars::shell(spec.name) {
                let text = format!("shell: {shell}");
                let style = if has_active_popup || state.basic_terminal {
                    dim
                } else {
                    theme.rating
                };
                vec![Span::styled(
                    crate::tui::text::truncate_middle_width(&text, budget),
                    style,
                )]
            } else if let Some(saved) = env_vars::saved(spec.name) {
                let style = if has_active_popup {
                    theme.muted
                } else if state.basic_terminal {
                    theme.text.add_modifier(Modifier::BOLD)
                } else {
                    theme.accent.add_modifier(Modifier::BOLD)
                };
                vec![Span::styled(
                    crate::tui::text::truncate_middle_width(&saved, budget),
                    style,
                )]
            } else {
                vec![Span::styled("default", dim)]
            };
        render_row(
            frame,
            row_area,
            SettingRow {
                is_selected,
                has_active_popup,
                label,
                value_spans,
            },
            theme,
            state.basic_terminal,
        );
    }
}

/// Help for the selected variable: what it does, when it applies, and the keys.
fn render_env_footer(
    frame: &mut Frame,
    area: Rect,
    popup_area: Rect,
    state: &AppState,
    theme: &Theme,
) {
    use crate::env_vars::{Applies, SPECS};
    let Some(spec) = SPECS.get(state.settings_selected_row) else {
        return;
    };
    let area = Rect {
        x: popup_area.x + 4,
        width: popup_area.width.saturating_sub(8),
        ..area
    };
    let width = area.width as usize;
    let applies = match spec.applies {
        Applies::Now => "applies now",
        Applies::Restart => "applies after restart",
    };
    let shell_note = if crate::env_vars::shell(spec.name).is_some() {
        " · shell value wins"
    } else {
        ""
    };
    let keys = if state.settings_text_input.is_some() {
        format!("Enter save · Esc cancel · empty clears · {applies}")
    } else {
        format!("Enter edit · d clear · {applies}{shell_note}")
    };
    let lines = vec![
        Line::from(Span::styled(
            crate::tui::text::truncate_width(spec.help, width).into_owned(),
            theme.subtext1,
        )),
        Line::from(Span::styled(
            crate::tui::text::truncate_width(&keys, width).into_owned(),
            theme.text_dim,
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn settings_row_rects_in_area(area: Rect, count: usize) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(count);
    for i in 0..count {
        let y = area.y + i as u16;
        if y < area.bottom() {
            rects.push(Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            });
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn test_render_settings_modal_all_categories() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::mocha();

        for cat in SettingsCategory::ALL {
            let mut state = AppState {
                show_settings_popup: true,
                settings_category: cat,
                settings_selected_row: 0,
                ..Default::default()
            };

            terminal
                .draw(|frame| {
                    let area = frame.area();
                    draw(frame, area, &mut state, &theme);
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let rendered = (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");

            assert!(rendered.contains("Settings & Preferences"));
            assert!(rendered.contains(cat.title()));
        }
    }

    fn render_to_string(terminal: &mut Terminal<TestBackend>, state: &mut AppState) -> String {
        let theme = Theme::mocha();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw(frame, area, state, &theme);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn env_tab_scrolls_to_the_selected_row_and_clicks_map_back() {
        use crate::env_vars::SPECS;
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        let mut state = AppState {
            show_settings_popup: true,
            settings_category: SettingsCategory::EnvVars,
            settings_selected_row: 0,
            ..Default::default()
        };
        let rendered = render_to_string(&mut terminal, &mut state);
        assert!(rendered.contains("Env Variables"));
        assert!(rendered.contains("SEEK_PROXY_MAX_MB"));
        assert!(rendered.contains("Enter edit"));
        assert!(!rendered.contains("PREVIOUS_WORKER_URL"));

        let last = SPECS.len() - 1;
        state.settings_selected_row = last;
        let rendered = render_to_string(&mut terminal, &mut state);
        assert!(rendered.contains("PREVIOUS_WORKER_URL"));
        assert!(!rendered.contains("SEEK_PROXY_MAX_MB"));
        let scroll = state.settings_scroll.get();
        assert!(scroll > 0);

        // A click on the first visible row selects the row the list is scrolled to.
        let popup =
            overlay::settings_modal_layout(Rect::new(0, 0, 80, 16), SettingsCategory::EnvVars);
        let (shown_from, rects) =
            settings_row_rects(popup, SettingsCategory::EnvVars, scroll, last);
        assert_eq!(shown_from, scroll);
        assert_eq!(
            settings_row_at(
                popup,
                SettingsCategory::EnvVars,
                scroll,
                last,
                rects[0].x + 2,
                rects[0].y
            ),
            Some(scroll)
        );

        // Moving up one row inside the window doesn't scroll.
        state.settings_selected_row = last - 1;
        render_to_string(&mut terminal, &mut state);
        assert_eq!(state.settings_scroll.get(), scroll);
    }

    #[test]
    fn all_five_tabs_fit_or_use_short_titles() {
        for width in [60u16, 80, 120] {
            let area = Rect::new(0, 0, width, 30);
            let popup = overlay::settings_modal_layout(area, SettingsCategory::General);
            let tabs = Rect {
                x: popup.x + 3,
                y: popup.y + 1,
                width: popup.width.saturating_sub(6),
                height: 2,
            };
            assert_eq!(
                category_tab_rects(tabs, false, SettingsCategory::General).len(),
                5,
                "width {width}"
            );
        }
    }

    #[test]
    fn test_render_settings_modal_basic_terminal() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::fallback(false);
        let mut state = AppState {
            show_settings_popup: true,
            basic_terminal: true,
            settings_category: SettingsCategory::General,
            settings_selected_row: 0,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw(frame, area, &mut state, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Settings & Preferences"));
        assert!(rendered.contains("Automatic Updates"));
    }

    #[test]
    fn test_render_settings_modal_compact_layout() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::mocha();
        let mut state = AppState {
            show_settings_popup: true,
            settings_category: SettingsCategory::General,
            settings_selected_row: 0,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw(frame, area, &mut state, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Settings & Preferences"));
        assert!(rendered.contains("Automatic Updates"));
    }

    #[test]
    fn test_render_content_modes_labels() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::mocha();
        let mut state = AppState {
            show_settings_popup: true,
            settings_category: SettingsCategory::ContentModes,
            settings_selected_row: 0,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw(frame, area, &mut state, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Streaming Mode"));
        assert!(rendered.contains("Streaming Sources"));
        assert!(rendered.contains("Live TV Mode"));
        assert!(!rendered.contains("Stremio Addons"));
    }

    #[test]
    fn test_settings_tab_and_row_hit_testing() {
        let popup = Rect::new(4, 4, 76, 17);
        let cat = settings_category_tab_at(popup, 7, popup.y + 1, false, SettingsCategory::General);
        assert_eq!(cat, Some(SettingsCategory::General));
        let cat_underline =
            settings_category_tab_at(popup, 7, popup.y + 2, false, SettingsCategory::General);
        assert_eq!(cat_underline, Some(SettingsCategory::General));

        let cat_modes =
            settings_category_tab_at(popup, 19, popup.y + 1, false, SettingsCategory::General);
        assert_eq!(cat_modes, Some(SettingsCategory::ContentModes));

        let (_, row_rects) = settings_row_rects(popup, SettingsCategory::General, 0, 0);
        assert_eq!(row_rects.len(), 3);

        let clicked_row =
            settings_row_at(popup, SettingsCategory::General, 0, 0, 40, row_rects[0].y);
        assert_eq!(clicked_row, Some(0));

        let clicked_row1 =
            settings_row_at(popup, SettingsCategory::General, 0, 0, 40, row_rects[1].y);
        assert_eq!(clicked_row1, Some(1));

        let compact_popup = Rect::new(2, 2, 54, 16);
        assert_eq!(
            settings_category_tab_at(compact_popup, 6, 3, false, SettingsCategory::General),
            Some(SettingsCategory::General)
        );
        assert_eq!(
            settings_category_tab_at(compact_popup, 14, 3, false, SettingsCategory::General),
            Some(SettingsCategory::ContentModes)
        );
        assert_eq!(
            settings_category_tab_at(compact_popup, 23, 3, false, SettingsCategory::General),
            Some(SettingsCategory::Appearance)
        );
        assert_eq!(
            settings_category_tab_at(compact_popup, 32, 3, false, SettingsCategory::General),
            Some(SettingsCategory::StorageInfo)
        );
    }
    #[test]
    fn test_render_settings_modal_suppresses_selection_during_popup() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::mocha();
        let mut state = AppState {
            show_settings_popup: true,
            settings_player_picker: true,
            settings_category: SettingsCategory::General,
            settings_selected_row: 1,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw(frame, area, &mut state, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Default Media Player"));
        assert!(!rendered.contains("▸ Default Media Player"));
    }
    #[test]
    fn test_render_appearance_labels() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::mocha();
        let mut state = AppState {
            show_settings_popup: true,
            settings_category: SettingsCategory::Appearance,
            settings_selected_row: 0,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw(frame, area, &mut state, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Theme"));

        let popup = Rect::new(4, 4, 76, 17);
        let (_, rows) = settings_row_rects(popup, SettingsCategory::Appearance, 0, 0);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn tail_width_keeps_the_end_of_a_long_pasted_cookie() {
        let cookie = "user=abcdefghijklmnopqrstuvwxyz; session=0123456789";
        let shown = crate::tui::text::tail_width(cookie, 20);
        assert!(crate::tui::text::width(&shown) <= 20);
        assert!(shown.starts_with('…'));
        assert!(shown.ends_with("session=0123456789"));
        assert_eq!(crate::tui::text::tail_width("short", 20), "short");
    }

    #[test]
    fn format_age_uses_the_largest_whole_unit() {
        assert_eq!(format_age(5), "just now");
        assert_eq!(format_age(5 * 60), "5m ago");
        assert_eq!(format_age(3 * 3600 + 59), "3h ago");
        assert_eq!(format_age(2 * 86_400), "2d ago");
    }

    #[test]
    fn test_settings_download_input_multibyte_utf8_cursor_split() {
        use crate::tui::text::TextInputBuffer;
        let mut input = TextInputBuffer::from_str("C:\\Users\\山田\\Downloads");
        input.set_cursor(11);
        let offset = input.cursor_byte_offset();
        let (before, after) = input.as_str().split_at(offset);
        assert_eq!(before, "C:\\Users\\山田");
        assert_eq!(after, "\\Downloads");
    }
}
