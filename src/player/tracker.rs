use std::fs;
use std::path::PathBuf;

const TRACKER_LUA_CONTENT: &str = r#"local options = require 'mp.options'

local opts = {
    provider = "",
    subject_id = "",
    season = 0,
    episode = 0,
    state_file = "",
    upnext_file = "",
}
options.read_options(opts, "moviebox")

local last_pos = 0
local last_dur = 0
local has_completed = false
local meta_title = ""
local meta_cover = ""
local meta_stype = 0
local meta_year = ""
local meta_stream = ""
local seed_read = false
local last_written_pos = -1
local last_written_completed = false
local function read_seed_meta()
    if seed_read or opts.state_file == "" then return end
    seed_read = true
    local f = io.open(opts.state_file, "r")
    if not f then return end
    local content = f:read("*a")
    f:close()
    if not content then return end
    meta_title = content:match('"title":%s*"([^"]-)"') or ""
    meta_cover = content:match('"cover_url":%s*"([^"]-)"') or ""
    local st = content:match('"stype":%s*(%d+)')
    if st then meta_stype = tonumber(st) or 0 end
    meta_year = content:match('"release_year":%s*"([^"]-)"') or ""
    meta_stream = content:match('"stream_filename":%s*"([^"]-)"') or ""
end

mp.observe_property("time-pos", "number", function(name, val)
    if val then
        last_pos = val
    end
end)

mp.observe_property("duration", "number", function(name, val)
    if val then
        last_dur = val
    end
end)

local function write_state(force_completed)
    if opts.state_file == "" then return end
    if last_dur <= 0 and last_pos <= 0 then return end

    if force_completed then
        has_completed = true
    end
    if last_dur > 0 and last_pos >= (0.90 * last_dur) then
        has_completed = true
    end

    local pos_floor = math.floor(last_pos + 0.5)
    local is_paused = mp.get_property_native("pause") == true
    if not force_completed and is_paused and pos_floor == last_written_pos and has_completed == last_written_completed then
        return
    end
    if not force_completed and math.abs(pos_floor - last_written_pos) < 1 and has_completed == last_written_completed then
        return
    end

    read_seed_meta()
    local dur_val = "null"
    if last_dur > 0 then
        dur_val = string.format("%d", math.floor(last_dur + 0.5))
    end

    local meta_json = ""
    if meta_title ~= "" then
        meta_json = string.format(
            ',"title":%q,"cover_url":%q,"stype":%d,"release_year":%q',
            meta_title,
            meta_cover,
            meta_stype,
            meta_year
        )
    end
    if meta_stream ~= "" then
        meta_json = meta_json .. string.format(',"stream_filename":%q', meta_stream)
    end

    local now = os.time()
    local json = string.format(
        '{"provider":%q,"subject_id":%q,"season":%d,"episode":%d,"progress_seconds":%d,"duration_seconds":%s,"completed":%s,"timestamp":%d%s}',
        opts.provider,
        opts.subject_id,
        tonumber(opts.season) or 0,
        tonumber(opts.episode) or 0,
        math.floor(last_pos + 0.5),
        dur_val,
        has_completed and "true" or "false",
        now,
        meta_json
    )

    local tmp_file = opts.state_file .. ".tmp"
    local f = io.open(tmp_file, "w")
    if f then
        f:write(json)
        f:flush()
        f:close()
        os.remove(opts.state_file)
        os.rename(tmp_file, opts.state_file)
        last_written_pos = pos_floor
        last_written_completed = has_completed
    end
end

mp.observe_property("pause", "bool", function(name, val)
    if val then
        write_state(false)
    end
end)

mp.register_event("end-file", function(event)
    if event and event.reason == "eof" then
        write_state(true)
    end
end)

mp.register_event("shutdown", function()
    write_state(false)
end)

mp.add_periodic_timer(5, function()
    write_state(false)
end)
"#;

pub fn ensure_tracker_script() -> Option<PathBuf> {
    let dir = crate::config::scripts_dir()?;
    if !dir.exists() {
        if let Err(e) = fs::create_dir_all(&dir) {
            log::warn!(
                "failed to create scripts dir {}: {e}",
                crate::logging::sanitize_path(&dir)
            );
            return None;
        }
    }
    let path = dir.join("moviebox_tracker.lua");
    if !path.exists()
        || fs::read_to_string(&path)
            .map(|c| c != TRACKER_LUA_CONTENT)
            .unwrap_or(true)
    {
        if let Err(e) = fs::write(&path, TRACKER_LUA_CONTENT.as_bytes()) {
            log::warn!(
                "failed to write tracker script {}: {e}",
                crate::logging::sanitize_path(&path)
            );
            return None;
        }
    }
    Some(path)
}

pub fn state_file_path(
    provider: &str,
    subject_id: &str,
    season: usize,
    episode: usize,
) -> Option<PathBuf> {
    let dir = crate::config::playback_state_dir()?;
    if !dir.exists() {
        if let Err(e) = fs::create_dir_all(&dir) {
            log::warn!(
                "failed to create playback state dir {}: {e}",
                crate::logging::sanitize_path(&dir)
            );
            return None;
        }
    }
    let sanitized_provider = sanitize_component(provider);
    let sanitized_id = sanitize_component(subject_id);
    let filename = format!("{sanitized_provider}_{sanitized_id}_{season}_{episode}.json");
    Some(dir.join(filename))
}

fn sanitize_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .take(120)
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' '
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches(['.', ' ', '_']);
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_component;

    #[test]
    fn state_file_components_are_safe_for_filenames() {
        assert_eq!(
            sanitize_component("provider/with\\separators"),
            "provider_with_separators"
        );
        assert_eq!(
            sanitize_component("subject:with space"),
            "subject_with_space"
        );
        assert_eq!(
            sanitize_component("subject*with?forbidden<chars>|quote\""),
            "subject_with_forbidden_chars__quote"
        );
    }

    #[test]
    fn lua_tracker_defines_now_before_formatting_json() {
        assert!(super::TRACKER_LUA_CONTENT.contains("local now = os.time()"));
    }
}
