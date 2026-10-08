fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

pub fn uses_basic_ui() -> bool {
    let term = env("TERM");
    if term == "dumb" || term == "linux" {
        return true;
    }
    crate::tui::theme::ColorSupport::current() == crate::tui::theme::ColorSupport::Basic
}
fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

fn outer_terminal_supports_graphics() -> bool {
    if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok() {
        return true;
    }
    if std::env::var("KITTY_WINDOW_ID").is_ok() {
        return true;
    }
    if std::env::var("WEZTERM_EXECUTABLE").is_ok() {
        return true;
    }
    if std::env::var("ITERM_SESSION_ID").is_ok() {
        return true;
    }
    if std::env::var("ALACRITTY_LOG").is_ok() || std::env::var("ALACRITTY_WINDOW_ID").is_ok() {
        return true;
    }
    let term = env("TERM");
    if term.starts_with("foot") {
        return true;
    }
    false
}

pub fn should_query_images() -> bool {
    if crate::env_vars::var("MOVIEBOX_NO_IMAGE")
        .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    {
        return false;
    }
    if let Ok(forced) = crate::env_vars::var("MOVIEBOX_IMAGE_PROTOCOL") {
        let forced = forced.trim();
        if forced.eq_ignore_ascii_case("none")
            || forced.eq_ignore_ascii_case("off")
            || forced.eq_ignore_ascii_case("false")
        {
            return false;
        }
        if !forced.is_empty() {
            return true;
        }
    }
    if is_inside_tmux()
        && crate::env_vars::var("MOVIEBOX_IMAGE_PROTOCOL").is_err()
        && !outer_terminal_supports_graphics()
    {
        return false;
    }
    if std::env::var("TERM_PROGRAM").is_ok_and(|v| v == "Apple_Terminal") {
        return false;
    }
    let term = env("TERM");
    if term == "dumb"
        || term == "linux"
        || term == "cygwin"
        || term.starts_with("vt")
        || term.starts_with("cons")
    {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        let is_modern_terminal = std::env::var("WT_SESSION").is_ok()
            || std::env::var("TERM_PROGRAM").is_ok()
            || std::env::var("ALACRITTY_LOG").is_ok()
            || std::env::var("WEZTERM_EXECUTABLE").is_ok()
            || std::env::var("GHOSTTY_RESOURCES_DIR").is_ok();
        if !is_modern_terminal {
            return false;
        }
    }
    true
}

pub fn background_is_light() -> bool {
    if let Ok(value) = std::env::var("COLORFGBG")
        && let Some(background) = value
            .split([';', ':'])
            .next_back()
            .and_then(|value| value.parse::<u8>().ok())
    {
        return matches!(background, 7 | 10..=15);
    }

    std::env::var("TERM_BACKGROUND")
        .or_else(|_| std::env::var("BACKGROUND"))
        .is_ok_and(|value| value.eq_ignore_ascii_case("light"))
}

pub fn set_window_title(title: &str) -> std::io::Result<()> {
    crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle(title))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_query_images_guards() {
        unsafe {
            std::env::set_var("MOVIEBOX_NO_IMAGE", "1");
            assert!(!should_query_images());
            std::env::remove_var("MOVIEBOX_NO_IMAGE");

            std::env::set_var("MOVIEBOX_IMAGE_PROTOCOL", "none");
            assert!(!should_query_images());
            std::env::remove_var("MOVIEBOX_IMAGE_PROTOCOL");

            std::env::set_var("TERM_PROGRAM", "Apple_Terminal");
            assert!(!should_query_images());
            std::env::remove_var("TERM_PROGRAM");

            std::env::set_var("TERM", "dumb");
            assert!(!should_query_images());
            std::env::remove_var("TERM");
            std::env::set_var("MOVIEBOX_IMAGE_PROTOCOL", "kitty");
            assert!(should_query_images());
            std::env::remove_var("MOVIEBOX_IMAGE_PROTOCOL");
            let saved_ghostty = std::env::var("GHOSTTY_RESOURCES_DIR").ok();
            let saved_kitty = std::env::var("KITTY_WINDOW_ID").ok();
            let saved_wezterm = std::env::var("WEZTERM_EXECUTABLE").ok();
            let saved_iterm = std::env::var("ITERM_SESSION_ID").ok();
            let saved_alacritty_log = std::env::var("ALACRITTY_LOG").ok();
            let saved_alacritty_win = std::env::var("ALACRITTY_WINDOW_ID").ok();

            std::env::remove_var("GHOSTTY_RESOURCES_DIR");
            std::env::remove_var("KITTY_WINDOW_ID");
            std::env::remove_var("WEZTERM_EXECUTABLE");
            std::env::remove_var("ITERM_SESSION_ID");
            std::env::remove_var("ALACRITTY_LOG");
            std::env::remove_var("ALACRITTY_WINDOW_ID");

            std::env::set_var("TMUX", "/tmp/tmux-1000/default,12345,0");
            assert!(!should_query_images());

            std::env::set_var("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty");
            assert!(should_query_images());
            std::env::remove_var("GHOSTTY_RESOURCES_DIR");

            std::env::set_var("MOVIEBOX_IMAGE_PROTOCOL", "kitty");
            assert!(should_query_images());
            std::env::remove_var("MOVIEBOX_IMAGE_PROTOCOL");
            std::env::remove_var("TMUX");

            if let Some(v) = saved_ghostty {
                std::env::set_var("GHOSTTY_RESOURCES_DIR", v);
            }
            if let Some(v) = saved_kitty {
                std::env::set_var("KITTY_WINDOW_ID", v);
            }
            if let Some(v) = saved_wezterm {
                std::env::set_var("WEZTERM_EXECUTABLE", v);
            }
            if let Some(v) = saved_iterm {
                std::env::set_var("ITERM_SESSION_ID", v);
            }
            if let Some(v) = saved_alacritty_log {
                std::env::set_var("ALACRITTY_LOG", v);
            }
            if let Some(v) = saved_alacritty_win {
                std::env::set_var("ALACRITTY_WINDOW_ID", v);
            }
        }
    }
}
