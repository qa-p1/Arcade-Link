-- Headless Hyprland for Arcade's desktop end-to-end tests (specs/LINK.md §1).
-- Matches the owner's defaults where behaviour depends on them (follow_mouse = 1).

hl.monitor({ output = "HEADLESS-1", mode = "1920x1080@60", position = "0x0", scale = "1" })
hl.monitor({ output = "HEADLESS-2", mode = "1920x1080@60", position = "1920x0", scale = "1" })

hl.config({
    input = { follow_mouse = 1, kb_layout = "us" },
    general = { gaps_in = 4, gaps_out = 8, border_size = 1, layout = "dwindle" },
    decoration = { rounding = 8, shadow = { enabled = false }, blur = { enabled = false } },
    animations = { enabled = false },
    misc = { disable_hyprland_logo = true, disable_splash_rendering = true },
    debug = { disable_logs = false, enable_stdout_logs = true },
})

hl.on("hyprland.start", function()
    hl.dispatch(hl.dsp.exec_cmd(os.getenv("ARCADE_DESKTOP_READY_CMD") or "true"))
end)
