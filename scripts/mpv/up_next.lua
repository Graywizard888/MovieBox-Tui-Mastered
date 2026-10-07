--[[
    Up Next Episodes Script for MPV / MPV Android
    Created by graywizard - v6.0 (MovieBox-TUI aware)

    Features:
    - Shows next episode when 2 minutes remain
    - Works with MovieBox-TUI network streams (reads an "up next" sidecar)
    - Works with local files (directory scan, same as v5.5)
    - Shows "Season Ended" only when there genuinely is no next episode
    - Autoplays the next episode when the current one ends
    - Live countdown timer with clock icon
    - Separator line above countdown
    - Properly handles seek/pause/resume
    - Right-aligned OSD with colorful multi-line text

    Episode sources, in priority order:
      1. MovieBox-TUI sidecar JSON (network streams; see SIDECAR_PATHS)
      2. MovieBox-TUI script-opts (desktop launches: moviebox-season/episode)
      3. Next video file in the same folder (local playback)
]]--

local mp = require 'mp'
local utils = require 'mp.utils'
local msg = require 'mp.msg'
local options = require 'mp.options'

--============== CONFIGURATION ==============--

local TRIGGER_SECONDS = 120          -- When to show OSD (seconds before end)
local STARTUP_DELAY = 2              -- Delay before reading duration
local COUNTDOWN_UPDATE_INTERVAL = 1  -- Countdown update frequency

local AUTOPLAY = true                -- Autoplay the next episode when this one ends
local AUTOPLAY_LEAD = 0              -- Start next episode N seconds before EOF (0 = at EOF)
local SIDECAR_POLL_INTERVAL = 5      -- Re-read sidecar every N seconds while waiting for a URL
local SIDECAR_MAX_AGE = 21600        -- Ignore sidecars older than this (seconds)

-- OSD Styling
local FONT_SIZE = 60
local ICON_SIZE = 55
local COUNTDOWN_SIZE = 48
local MARGIN_RIGHT = 50
local MARGIN_BOTTOM = 300
local BORDER_SIZE = 3
local MAX_NAME_LENGTH = 28

-- Colors (BGR format for ASS)
local COLOR_ICON = "FFFFFF"           -- White icon
local COLOR_LABEL = "00DDFF"          -- Cyan "Up Next:"
local COLOR_NAME = "00FF00"           -- Green for show name
local COLOR_EPISODE = "FFFFFF"        -- White for episode
local COLOR_ENDED = "6666FF"          -- Red for season ended
local COLOR_BORDER = "000000"         -- Black border
local COLOR_LINE = "888888"           -- Gray separator

-- Countdown colors
local COLOR_COUNTDOWN = "00A5FF"      -- Orange countdown
local COLOR_COUNTDOWN_LOW = "0055FF"  -- Red when below threshold
local COUNTDOWN_LOW_THRESHOLD = 30    -- Seconds to switch to red

--============== END CONFIGURATION ==============--

-- MovieBox-TUI passes these via --script-opts on desktop launches.
-- Android intent launches cannot pass script-opts, so the sidecar file
-- is discovered from the well-known paths below instead.
local opts = {
    provider = "",
    subject_id = "",
    season = 0,
    episode = 0,
    state_file = "",
    upnext_file = "",
    autoplay = true,
}
options.read_options(opts, "moviebox")

if opts.autoplay == false then
    AUTOPLAY = false
end

-- Well-known sidecar locations, tried in order. Android shared storage comes
-- first because mpv-android cannot read Termux's private data directory.
local function sidecar_candidates()
    local list = {}

    if opts.upnext_file ~= "" then
        list[#list + 1] = opts.upnext_file
    end

    list[#list + 1] = "/storage/emulated/0/MovieBox-TUI/upnext.json"
    list[#list + 1] = "/sdcard/MovieBox-TUI/upnext.json"

    local home = os.getenv("HOME")
    if home then
        list[#list + 1] = home .. "/.local/share/moviebox-tui/playback/upnext.json"
        list[#list + 1] = home .. "/Library/Application Support/moviebox-tui/playback/upnext.json"
        list[#list + 1] = home .. "/storage/shared/MovieBox-TUI/upnext.json"
    end

    local xdg = os.getenv("XDG_DATA_HOME")
    if xdg and xdg ~= "" then
        list[#list + 1] = xdg .. "/moviebox-tui/playback/upnext.json"
    end

    local appdata = os.getenv("APPDATA")
    if appdata and appdata ~= "" then
        list[#list + 1] = appdata .. "/moviebox-tui/playback/upnext.json"
    end

    return list
end

-- State
local trigger_timer = nil
local countdown_timer = nil
local sidecar_timer = nil
local video_duration = nil
local trigger_time = nil
local osd_active = false
local ready = false
local cached_next_file = nil
local cached_display_text = nil
local sidecar = nil           -- parsed sidecar table for the current episode
local autoplay_cancelled = false
local autoplay_fired = false
local keep_open_forced = false

-- Reset all state
local function reset_state()
    if trigger_timer then
        trigger_timer:kill()
        trigger_timer = nil
    end
    if countdown_timer then
        countdown_timer:kill()
        countdown_timer = nil
    end
    if sidecar_timer then
        sidecar_timer:kill()
        sidecar_timer = nil
    end
    video_duration = nil
    trigger_time = nil
    osd_active = false
    ready = false
    cached_next_file = nil
    cached_display_text = nil
    sidecar = nil
    autoplay_cancelled = false
    autoplay_fired = false
end

-- Clear OSD display
local function clear_osd()
    mp.set_osd_ass(0, 0, "")
    osd_active = false
    if countdown_timer then
        countdown_timer:kill()
        countdown_timer = nil
    end
end

-- Format seconds to MM:SS
local function format_time(seconds)
    if not seconds or seconds < 0 then seconds = 0 end
    local mins = math.floor(seconds / 60)
    local secs = math.floor(seconds % 60)
    return string.format("%02d:%02d", mins, secs)
end

local function read_file(path)
    if not path or path == "" then return nil end
    local f = io.open(path, "r")
    if not f then return nil end
    local content = f:read("*a")
    f:close()
    if not content or content == "" then return nil end
    return content
end

local function write_file(path, content)
    if not path or path == "" then return false end
    local f = io.open(path, "w")
    if not f then return false end
    f:write(content)
    f:close()
    return true
end

local function now_seconds()
    return os.time()
end

-- Is this sidecar describing the episode we are currently playing?
local function sidecar_matches_current(data)
    if type(data) ~= "table" then return false end

    if data.updated_at and SIDECAR_MAX_AGE > 0 then
        local age = now_seconds() - data.updated_at
        if age > SIDECAR_MAX_AGE then
            msg.verbose("ignoring stale up-next sidecar (age " .. tostring(age) .. "s)")
            return false
        end
    end

    -- When script-opts are available (desktop), require an exact match so a
    -- stale sidecar from a different show is never used.
    if opts.subject_id ~= "" and data.subject_id and data.subject_id ~= "" then
        if data.subject_id ~= opts.subject_id then
            return false
        end
        if data.current and tonumber(opts.episode) and tonumber(opts.episode) > 0 then
            if tonumber(data.current.episode) ~= tonumber(opts.episode) then
                return false
            end
        end
        return true
    end

    -- Android intent launches have no script-opts. Fall back to matching the
    -- stream URL the app recorded for the episode it just launched.
    local path = mp.get_property("path")
    if path and data.current and data.current.url and data.current.url ~= "" then
        if path == data.current.url then
            return true
        end
    end

    -- Last resort: trust a very fresh sidecar (the app writes it immediately
    -- before launching the player).
    if data.updated_at and (now_seconds() - data.updated_at) <= 120 then
        return true
    end

    return false
end

-- Load the MovieBox-TUI sidecar, if one applies to this playback session
local function load_sidecar()
    for _, path in ipairs(sidecar_candidates()) do
        local content = read_file(path)
        if content then
            local data = utils.parse_json(content)
            if data and sidecar_matches_current(data) then
                data._path = path
                msg.info("using up-next sidecar: " .. path)
                return data
            end
        end
    end
    return nil
end

-- Check if file is video
local function is_video(file)
    local ext = file:lower():match("%.(%w+)$")
    if not ext then return false end
    local video_exts = {
        mkv=1, mp4=1, avi=1, webm=1, m4v=1, mov=1,
        flv=1, wmv=1, ts=1, m2ts=1
    }
    return video_exts[ext] == 1
end

-- Extract show name and episode (local files)
local function extract_name_and_episode(filename)
    if not filename then return nil, nil end

    local name = filename:gsub("%.[^%.]+$", "")
    name = name:gsub("%[.-%]", "")
    name = name:gsub("%(.-%)","")

    local show, episode = name:match("^%s*(.-)%s*([Ss]%d+[Ee]%d+)")
    if show and episode then
        show = show:gsub("^%s+", ""):gsub("%s+$", ""):gsub("%s+", " ")
        show = show:gsub("[%-%._]+$", ""):gsub("^%s+", ""):gsub("%s+$", "")
        return show, episode:upper()
    end

    local show2, episode2 = name:match("^%s*(.-)%s*%-?%s*([Ee][Pp]?%d+)")
    if show2 and episode2 then
        show2 = show2:gsub("^%s+", ""):gsub("%s+$", ""):gsub("%s+", " ")
        show2 = show2:gsub("[%-%._]+$", ""):gsub("^%s+", ""):gsub("%s+$", "")
        return show2, episode2:upper()
    end

    local show3, episode3 = name:match("^%s*(.-)%s*%-?%s*(%d%d+)")
    if show3 and episode3 then
        show3 = show3:gsub("^%s+", ""):gsub("%s+$", ""):gsub("%s+", " ")
        show3 = show3:gsub("[%-%._]+$", ""):gsub("^%s+", ""):gsub("%s+$", "")
        if show3 ~= "" then
            return show3, "EP" .. episode3
        end
    end

    local clean = name
    local tags = {
        "%d+p", "[Xx]%d+", "HEVC", "AVC", "10bit", "8bit",
        "WEB%-?DL", "WEBRip", "BluRay", "BRRip", "HDRip",
        "Multi%s*Audio", "Dual%s*Audio", "ESub", "EngSub",
        "AAC", "AC3", "DTS", "FLAC", "OPUS"
    }
    for _, tag in ipairs(tags) do
        clean = clean:gsub("%s*" .. tag .. "%s*", " ")
    end
    clean = clean:gsub("^%s+", ""):gsub("%s+$", ""):gsub("%s+", " ")

    return clean, nil
end

-- Find next video file in same folder (local playback only)
local function find_next_file()
    if cached_next_file ~= nil then
        return cached_next_file
    end

    local path = mp.get_property("path")
    if not path then
        cached_next_file = false
        return false
    end

    -- Network streams have no directory to scan. This is exactly the case
    -- MovieBox-TUI hits, and why v5.5 always reported "Season Ended".
    if path:match("^%a[%w+.-]*://") then
        cached_next_file = false
        return false
    end

    local dir, current = utils.split_path(path)
    if not dir or not current then
        cached_next_file = false
        return false
    end

    if dir:match("^content://") then
        cached_next_file = false
        return false
    end

    local entries = utils.readdir(dir, "files")
    if not entries then
        cached_next_file = false
        return false
    end

    local videos = {}
    for _, f in ipairs(entries) do
        if is_video(f) then
            videos[#videos + 1] = f
        end
    end
    table.sort(videos, function(a, b)
        return a:lower() < b:lower()
    end)

    for i, f in ipairs(videos) do
        if f == current then
            cached_next_file = videos[i + 1] or false
            return cached_next_file
        end
    end

    cached_next_file = false
    return false
end

-- Read show title out of the tracker state file (desktop fallback)
local function title_from_state_file()
    local content = read_file(opts.state_file)
    if not content then return nil end
    return content:match('"title":%s*"([^"]-)"')
end

-- Build the next-episode descriptor from the best source available
local function get_display_text()
    if cached_display_text then
        return cached_display_text
    end

    -- 1. MovieBox-TUI sidecar (network streams)
    if sidecar == nil then
        sidecar = load_sidecar() or false
    end

    if sidecar and sidecar.next then
        local nxt = sidecar.next
        if nxt.has_next then
            local label = nxt.label
            if not label or label == "" then
                label = string.format("S%02dE%02d",
                    tonumber(nxt.season) or 0, tonumber(nxt.episode) or 0)
            end
            cached_display_text = {
                show_name = nxt.title or sidecar.title or "Next Episode",
                episode = label,
                has_next = true,
                source = "sidecar",
                url = nxt.url,
                subtitle = nxt.subtitle,
                headers = nxt.headers,
                season = tonumber(nxt.season) or 0,
                episode_number = tonumber(nxt.episode) or 0,
            }
            return cached_display_text
        end

        -- Sidecar explicitly says this was the final episode.
        cached_display_text = { has_next = false, source = "sidecar" }
        return cached_display_text
    end

    -- 2. script-opts only (desktop launch, no sidecar yet): we know the current
    -- episode number, so we can at least name the next one.
    if opts.subject_id ~= "" and tonumber(opts.episode) and tonumber(opts.episode) > 0 then
        local season = tonumber(opts.season) or 1
        local episode = tonumber(opts.episode) + 1
        cached_display_text = {
            show_name = title_from_state_file() or "Next Episode",
            episode = string.format("S%02dE%02d", season, episode),
            has_next = true,
            source = "script-opts",
            season = season,
            episode_number = episode,
        }
        return cached_display_text
    end

    -- 3. Local file: scan the folder (original v5.5 behaviour)
    local next_file = find_next_file()
    if next_file then
        local show_name, episode = extract_name_and_episode(next_file)
        if not show_name or show_name == "" then
            show_name = next_file:gsub("%.[^%.]+$", "")
        end
        local dir = utils.split_path(mp.get_property("path") or "")
        cached_display_text = {
            show_name = show_name,
            episode = episode,
            has_next = true,
            source = "local",
            url = dir and (dir .. next_file) or next_file,
        }
    else
        cached_display_text = {
            show_name = nil,
            episode = nil,
            has_next = false,
            source = "none",
        }
    end

    return cached_display_text
end

-- Truncate text if too long
local function truncate_text(text, max_len)
    if not text then return "" end
    if #text <= max_len then return text end
    return text:sub(1, max_len - 3) .. "..."
end

--============== AUTOPLAY ==============--

-- Ask MovieBox-TUI to play the next episode. Used when the app has not (yet)
-- resolved a direct URL for it: the TUI watches this file and takes over.
local function request_next_from_app(data)
    if not sidecar or not sidecar.request_file or sidecar.request_file == "" then
        return false
    end
    local payload = string.format(
        '{"provider":"%s","subject_id":"%s","season":%d,"episode":%d,"requested_at":%d}',
        sidecar.provider or opts.provider or "",
        sidecar.subject_id or opts.subject_id or "",
        data.season or 0,
        data.episode_number or 0,
        now_seconds()
    )
    if write_file(sidecar.request_file, payload) then
        msg.info("requested next episode from MovieBox-TUI")
        return true
    end
    msg.warn("could not write up-next request file: " .. tostring(sidecar.request_file))
    return false
end

local function apply_stream_headers(headers)
    if type(headers) ~= "table" then return end
    local fields = {}
    for _, pair in ipairs(headers) do
        local name, value = pair[1], pair[2]
        if type(pair) == "table" and name and value then
            if name:lower() == "user-agent" then
                mp.set_property("user-agent", value)
            elseif name:lower() == "referer" then
                mp.set_property("referrer", value)
            else
                fields[#fields + 1] = name .. ": " .. value
            end
        end
    end
    if #fields > 0 then
        mp.set_property_native("http-header-fields", fields)
    end
end

local function play_next(data)
    if autoplay_fired then return end
    autoplay_fired = true

    data = data or get_display_text()
    if not data.has_next then return end

    -- Direct URL available (local file, or a stream the app already resolved)
    if data.url and data.url ~= "" then
        apply_stream_headers(data.headers)
        if data.episode and data.episode ~= "" then
            local title = data.show_name or ""
            mp.set_property("force-media-title",
                (title ~= "" and (title .. " - ") or "") .. data.episode)
        end
        mp.osd_message("Playing next episode...", 3)
        mp.commandv("loadfile", data.url, "replace")
        if data.subtitle and data.subtitle ~= "" then
            local sub = data.subtitle
            mp.add_timeout(1.5, function()
                mp.commandv("sub-add", sub, "select")
            end)
        end
        return
    end

    -- No URL: hand the baton back to MovieBox-TUI, which will resolve the
    -- stream and relaunch the player.
    if request_next_from_app(data) then
        mp.osd_message("Loading next episode in MovieBox-TUI...", 4)
        mp.add_timeout(1.0, function()
            mp.commandv("quit")
        end)
    else
        mp.osd_message("No next episode source available", 3)
    end
end

local function cancel_autoplay()
    autoplay_cancelled = true
    if keep_open_forced then
        mp.set_property("keep-open", "no")
        keep_open_forced = false
    end
    clear_osd()
    mp.osd_message("Autoplay cancelled", 2)
end

-- Keep mpv alive at EOF so we get a chance to load the next episode.
-- MovieBox-TUI launches mpv with --keep-open=no, which would otherwise quit
-- before the script can react.
local function hold_open_for_autoplay()
    if not AUTOPLAY or keep_open_forced then return end
    local data = get_display_text()
    if not data.has_next then return end
    mp.set_property("keep-open", "yes")
    keep_open_forced = true
end

-- Re-read the sidecar while the episode plays: MovieBox-TUI may fill in the
-- resolved URL for the next episode after playback has already started.
local function start_sidecar_polling()
    if sidecar_timer or SIDECAR_POLL_INTERVAL <= 0 then return end
    sidecar_timer = mp.add_periodic_timer(SIDECAR_POLL_INTERVAL, function()
        if autoplay_fired then return end
        local fresh = load_sidecar()
        if fresh then
            local had_url = cached_display_text and cached_display_text.url
            sidecar = fresh
            cached_display_text = nil
            local data = get_display_text()
            if data.has_next and data.url and not had_url then
                msg.info("next episode URL resolved by MovieBox-TUI")
                hold_open_for_autoplay()
                if osd_active then
                    -- refresh the overlay with the real episode title
                    mp.add_timeout(0, function() end)
                end
            end
        end
    end)
end

--============== OSD ==============--

-- Show/Update OSD with countdown
local function show_osd()
    local data = get_display_text()

    local time_pos = mp.get_property_number("time-pos", 0)
    local remaining = video_duration - time_pos
    if remaining < 0 then remaining = 0 end

    local w = mp.get_property_number("osd-width", 1920)
    local h = mp.get_property_number("osd-height", 1080)

    local right_x = w - MARGIN_RIGHT
    local base_y = h - MARGIN_BOTTOM

    -- Choose countdown color (red when low)
    local countdown_color = remaining <= COUNTDOWN_LOW_THRESHOLD and COLOR_COUNTDOWN_LOW or COLOR_COUNTDOWN
    local countdown_text = format_time(remaining)

    local ass = ""

    if data.has_next then
        local display_name = truncate_text(data.show_name, MAX_NAME_LENGTH)
        local episode_text = data.episode or ""

        -- Base style
        ass = string.format(
            "{\\an3\\pos(%d,%d)\\bord%d\\shad0\\b1\\3c&H%s&}",
            right_x, base_y,
            BORDER_SIZE, COLOR_BORDER
        )

        -- Line 1: Play icon + Up Next:
        ass = ass .. string.format(
            "{\\fs%d\\c&H%s&}⏭  {\\c&H%s&}Up Next:\\N",
            ICON_SIZE, COLOR_ICON, COLOR_LABEL
        )

        -- Line 2: Show name
        ass = ass .. string.format(
            "{\\fs%d\\c&H%s&}%s\\N",
            FONT_SIZE, COLOR_NAME, display_name
        )

        -- Line 3: Episode
        if episode_text ~= "" then
            ass = ass .. string.format(
                "{\\fs%d\\c&H%s&}%s\\N",
                FONT_SIZE, COLOR_EPISODE, episode_text
            )
        end

        -- Line 4: Separator line
        ass = ass .. string.format(
            "{\\fs%d\\c&H%s&}─────────────────\\N",
            30, COLOR_LINE
        )

        -- Line 5: Clock icon + Countdown timer
        local label = (AUTOPLAY and not autoplay_cancelled) and "Starting in" or "Ends in"
        ass = ass .. string.format(
            "{\\fs%d\\c&H%s&}⏱ %s %s",
            COUNTDOWN_SIZE, countdown_color, label, countdown_text
        )

        if not osd_active then
            msg.info("Up Next: " .. tostring(data.show_name) .. " " .. (data.episode or "")
                .. " [" .. tostring(data.source) .. "]")
        end

    else
        -- Season Ended - with episode countdown
        ass = string.format(
            "{\\an3\\pos(%d,%d)\\bord%d\\shad0\\b1\\3c&H%s&}",
            right_x, base_y,
            BORDER_SIZE, COLOR_BORDER
        )

        -- Line 1: Stop icon + Season Ended:
        ass = ass .. string.format(
            "{\\fs%d\\c&H%s&}■  {\\c&H%s&}Season Ended\\N",
            ICON_SIZE, COLOR_ENDED, COLOR_ENDED
        )

        -- Line 2: Separator line
        ass = ass .. string.format(
            "{\\fs%d\\c&H%s&}─────────────────\\N",
            30, COLOR_LINE
        )

        -- Line 3: Clock icon + Episode ends in timer
        ass = ass .. string.format(
            "{\\fs%d\\c&H%s&}⏱ Episode ends in %s",
            COUNTDOWN_SIZE, countdown_color, countdown_text
        )

        if not osd_active then
            msg.info("Season Ended")
        end
    end

    mp.set_osd_ass(w, h, ass)
    osd_active = true

    -- Autoplay slightly before EOF when configured to do so
    if AUTOPLAY and not autoplay_cancelled and not autoplay_fired
        and AUTOPLAY_LEAD > 0 and remaining <= AUTOPLAY_LEAD and data.has_next then
        play_next(data)
    end
end

-- Start countdown timer
local function start_countdown_timer()
    if countdown_timer then
        countdown_timer:kill()
    end

    countdown_timer = mp.add_periodic_timer(COUNTDOWN_UPDATE_INTERVAL, function()
        if osd_active and not mp.get_property_bool("pause") then
            show_osd()
        end
    end)
end

-- Stop countdown timer
local function stop_countdown_timer()
    if countdown_timer then
        countdown_timer:kill()
        countdown_timer = nil
    end
end

-- Kill trigger timer
local function kill_timer()
    if trigger_timer then
        trigger_timer:kill()
        trigger_timer = nil
    end
end

-- Schedule trigger timer
local function schedule_timer()
    if not ready then return end

    kill_timer()

    if mp.get_property_bool("pause") then
        return
    end

    local time_pos = mp.get_property_number("time-pos")
    if not time_pos then return end

    if time_pos >= trigger_time then
        if not osd_active then
            show_osd()
            start_countdown_timer()
        end
        return
    end

    if osd_active then
        clear_osd()
        stop_countdown_timer()
    end

    local delay = trigger_time - time_pos

    if delay > 0 then
        trigger_timer = mp.add_timeout(delay, function()
            if not mp.get_property_bool("pause") and not osd_active then
                show_osd()
                start_countdown_timer()
            end
        end)
    end
end

-- Initialize after startup delay
local function initialize()
    video_duration = mp.get_property_number("duration")

    if not video_duration then
        return
    end

    if video_duration <= TRIGGER_SECONDS then
        trigger_time = 0
    else
        trigger_time = video_duration - TRIGGER_SECONDS
    end

    ready = true
    local data = get_display_text()
    if data.has_next then
        hold_open_for_autoplay()
        start_sidecar_polling()
    end
    schedule_timer()
end

-- Event: File loaded
local function on_file_loaded()
    reset_state()
    clear_osd()
    keep_open_forced = false
    mp.add_timeout(STARTUP_DELAY, initialize)
end

-- Event: End of file reached (autoplay hook)
local function on_eof()
    if not AUTOPLAY or autoplay_cancelled or autoplay_fired then
        if keep_open_forced then
            mp.set_property("keep-open", "no")
            keep_open_forced = false
            mp.commandv("quit")
        end
        return
    end
    local data = get_display_text()
    if data.has_next then
        play_next(data)
    elseif keep_open_forced then
        mp.set_property("keep-open", "no")
        keep_open_forced = false
        mp.commandv("quit")
    end
end

-- Event: File ended
local function on_end_file()
    clear_osd()
    stop_countdown_timer()
end

-- Event: Seek started
local function on_seek()
    if not ready then return end
    stop_countdown_timer()
end

-- Event: Playback restarted (after seek)
local function on_playback_restart()
    if not ready then return end

    local time_pos = mp.get_property_number("time-pos")
    if not time_pos then return end

    if time_pos >= trigger_time then
        if not osd_active then
            show_osd()
        end
        if not mp.get_property_bool("pause") then
            start_countdown_timer()
        end
    else
        if osd_active then
            clear_osd()
        end
        stop_countdown_timer()
    end

    schedule_timer()
end

-- Event: Pause state changed
local function on_pause_change(_, paused)
    if not ready then return end

    if paused then
        kill_timer()
        stop_countdown_timer()
    else
        if osd_active then
            show_osd()
            start_countdown_timer()
        end
        schedule_timer()
    end
end

-- Event: OSD dimensions changed
local function on_osd_dimensions_change()
    if osd_active then
        show_osd()
    end
end

-- Register events
mp.register_event("file-loaded", on_file_loaded)
mp.register_event("end-file", on_end_file)
mp.register_event("seek", on_seek)
mp.register_event("playback-restart", on_playback_restart)
mp.observe_property("pause", "bool", on_pause_change)
mp.observe_property("osd-width", "number", on_osd_dimensions_change)
mp.observe_property("osd-height", "number", on_osd_dimensions_change)
mp.observe_property("eof-reached", "bool", function(_, value)
    if value then on_eof() end
end)

-- Manual controls
mp.add_key_binding("ENTER", "upnext-play-now", function()
    local data = get_display_text()
    if data.has_next then
        play_next(data)
    else
        mp.osd_message("No next episode", 2)
    end
end)
mp.add_key_binding("c", "upnext-cancel", cancel_autoplay)
