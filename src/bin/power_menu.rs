use gtk4::prelude::*;
use serde_json::json;
use std::cell::Cell;
use std::env;
use std::fs;
use std::process::Command;
use std::rc::Rc;
use system_ui_helpers::*;

fn get_cmd_output(cmd: &[&str]) -> String {
    Command::new(cmd[0])
        .args(&cmd[1..])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn get_current_tlp_profile() -> String {
    let out = get_cmd_output(&["/run/current-system/sw/bin/tlpctl", "get"]);
    if out.is_empty() {
        get_cmd_output(&["tlpctl", "get"])
    } else {
        out
    }
}

fn is_nv_blocked() -> bool {
    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("nvidia") {
                if let Ok(meta) = entry.metadata() {
                    use std::os::unix::fs::PermissionsExt;
                    if (meta.permissions().mode() & 0o777) == 0 {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn get_current_lact_profile() -> String {
    let out = get_cmd_output(&["lact", "cli", "profile", "get"]).to_lowercase();
    if out.contains("powersave") {
        "Powersave".into()
    } else if out.contains("performance") {
        "Performance".into()
    } else {
        "Balanced".into()
    }
}

fn get_fan_mode() -> String {
    fs::read_to_string("/sys/devices/platform/aorus_laptop/fan_mode")
        .map(|s| match s.trim() {
            "5" => "max",
            "3" => "quiet",
            _ => "auto",
        })
        .unwrap_or("auto")
        .into()
}

fn get_hypr_animations() -> bool {
    let out = get_cmd_output(&["hyprctl", "getoption", "animations:enabled", "-j"]);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) {
        if let Some(i) = v.get("int").and_then(|i| i.as_i64()) {
            return i == 1;
        }
        if let Some(b) = v.get("bool").and_then(|b| b.as_bool()) {
            return b;
        }
    }
    true
}

fn is_edp_60hz() -> bool {
    let out = get_cmd_output(&["hyprctl", "monitors", "-j"]);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) {
        if let Some(arr) = v.as_array() {
            for m in arr {
                if m.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .starts_with("eDP-")
                {
                    if let Some(rate) = m.get("refreshRate").and_then(|r| r.as_f64()) {
                        return rate > 58.0 && rate < 61.0;
                    }
                }
            }
        }
    }
    false
}

fn is_replays_running() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "replays"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn apply_software_powersave(anim: bool, low_fps: bool, replays: bool) {
    if anim || !low_fps {
        Command::new("hyprctl").arg("reload").spawn().ok();
    }
    if !anim {
        Command::new("hyprctl")
            .args([
                "eval",
                "hl.config { animations = { enabled = 0 }, general = { border_size = 0 } }",
            ])
            .spawn()
            .ok();
    }
    if low_fps {
        let monitors = get_cmd_output(&["hyprctl", "monitors", "-j"]);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&monitors) {
            if let Some(arr) = v.as_array() {
                if let Some(m) = arr.iter().find(|m| {
                    m.get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .starts_with("eDP-")
                }) {
                    let name = m["name"].as_str().unwrap_or("eDP-1");
                    let w = m["width"].as_i64().unwrap_or(2560);
                    let h = m["height"].as_i64().unwrap_or(1600);
                    Command::new("hyprctl")
                        .args([
                            "eval",
                            &format!(
                                "hl.monitor({{ output = \"{}\", mode = \"{}x{}@60\", position = \"auto\", scale = \"auto\", bitdepth = 10 }})",
                                name, w, h
                            ),
                        ])
                        .spawn()
                        .ok();
                }
            }
        }
    }
    Command::new("systemctl")
        .args(["--user", if replays { "start" } else { "stop" }, "replays"])
        .spawn()
        .ok();
    Command::new("pkill")
        .args(["-RTMIN+5", "waybar"])
        .spawn()
        .ok();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a.to_lowercase().contains("getdata")) {
        let prof = get_current_tlp_profile();
        let data = if prof.contains("power-saver") {
            json!({"text": "󰌪", "class": "powersave", "tooltip": "Mode: Power Saver"})
        } else if prof.contains("performance") {
            json!({"text": "󰓅", "class": "performance", "tooltip": "Mode: Performance"})
        } else {
            json!({"text": "󰗑", "class": "default", "tooltip": "Mode: Balanced"})
        };
        println!("{}", data);
        std::process::exit(0);
    }

    gtk4::init().expect("GTK4 init failed");
    apply_clean_theme();
    let main_loop = glib::MainLoop::new(None, false);

    let win = gtk4::Window::builder()
        .title("Power menu")
        .default_width(480)
        .resizable(false)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    vbox.set_margin_top(15);
    vbox.set_margin_bottom(15);
    vbox.set_margin_start(15);
    vbox.set_margin_end(15);
    win.set_child(Some(&vbox));

    let title_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let img = gtk4::Image::from_icon_name("battery-full-symbolic");
    img.set_pixel_size(32);
    img.set_valign(gtk4::Align::Center);
    let lbl_title = gtk4::Label::builder()
        .label("<span size='large' weight='bold'>Power menu</span>")
        .use_markup(true)
        .build();
    title_box.append(&img);
    title_box.append(&lbl_title);
    vbox.append(&title_box);
    vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let lbl_hw_title = gtk4::Label::builder()
        .label("<span weight='bold'>Hardware Power Settings</span>")
        .use_markup(true)
        .xalign(0.0)
        .build();
    lbl_hw_title.set_margin_top(5);
    vbox.append(&lbl_hw_title);

    let hw_grid = gtk4::Grid::new();
    hw_grid.set_column_spacing(20);
    hw_grid.set_row_spacing(15);
    hw_grid.set_margin_top(5);
    hw_grid.set_margin_bottom(5);
    vbox.append(&hw_grid);

    let lbl_nv = gtk4::Label::builder()
        .use_markup(true)
        .label(
            "<b>Block NVIDIA GPU:</b>\n<span size='small' color='gray'>nv-blindfold wrapper</span>",
        )
        .xalign(0.0)
        .yalign(0.5)
        .build();
    let sw_nv = gtk4::Switch::new();
    sw_nv.set_valign(gtk4::Align::Center);
    sw_nv.set_halign(gtk4::Align::End);
    sw_nv.set_active(is_nv_blocked());
    sw_nv.connect_state_set(|_, state| {
        Command::new("/run/wrappers/bin/nv-blindfold")
            .arg(if state { "block" } else { "unblock" })
            .spawn()
            .ok();
        Command::new("pkill")
            .args(["-RTMIN+5", "waybar"])
            .spawn()
            .ok();
        glib::Propagation::Proceed
    });
    hw_grid.attach(&lbl_nv, 0, 0, 1, 1);
    hw_grid.attach(&sw_nv, 1, 0, 1, 1);

    let lbl_tlp = gtk4::Label::builder()
        .use_markup(true)
        .label("<b>TLP Power Profile:</b>\n<span size='small' color='gray'>System CPU power scaling</span>")
        .xalign(0.0)
        .yalign(0.5)
        .build();
    let tlp_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    tlp_box.add_css_class("linked");
    tlp_box.set_valign(gtk4::Align::Center);
    tlp_box.set_halign(gtk4::Align::End);
    let tlp_s = gtk4::ToggleButton::builder().label("Saver").build();
    let tlp_b = gtk4::ToggleButton::builder()
        .label("Balanced")
        .group(&tlp_s)
        .build();
    let tlp_p = gtk4::ToggleButton::builder()
        .label("Performance")
        .group(&tlp_s)
        .build();
    tlp_box.append(&tlp_s);
    tlp_box.append(&tlp_b);
    tlp_box.append(&tlp_p);

    let cur_tlp = get_current_tlp_profile();
    if cur_tlp.contains("power-saver") {
        tlp_s.set_active(true);
    } else if cur_tlp.contains("performance") {
        tlp_p.set_active(true);
    } else {
        tlp_b.set_active(true);
    }

    tlp_s.connect_toggled(|c| {
        if c.is_active() {
            Command::new("tlpctl")
                .args(["set", "power-saver"])
                .spawn()
                .ok();
            Command::new("pkill")
                .args(["-RTMIN+5", "waybar"])
                .spawn()
                .ok();
        }
    });
    tlp_b.connect_toggled(|c| {
        if c.is_active() {
            Command::new("tlpctl")
                .args(["set", "balanced"])
                .spawn()
                .ok();
            Command::new("pkill")
                .args(["-RTMIN+5", "waybar"])
                .spawn()
                .ok();
        }
    });
    tlp_p.connect_toggled(|c| {
        if c.is_active() {
            Command::new("tlpctl")
                .args(["set", "performance"])
                .spawn()
                .ok();
            Command::new("ryzenadj")
                .args([
                    "--stapm-limit=999999999999999999",
                    "--fast-limit=999999999999999999",
                    "--slow-limit=999999999999999999",
                ])
                .spawn()
                .ok();
            Command::new("pkill")
                .args(["-RTMIN+5", "waybar"])
                .spawn()
                .ok();
        }
    });
    hw_grid.attach(&lbl_tlp, 0, 1, 1, 1);
    hw_grid.attach(&tlp_box, 1, 1, 1, 1);

    let lbl_lact = gtk4::Label::builder()
        .use_markup(true)
        .label("<b>LACT GPU Profile:</b>\n<span size='small' color='gray'>Radeon power profiles</span>")
        .xalign(0.0)
        .yalign(0.5)
        .build();
    let lact_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    lact_box.add_css_class("linked");
    lact_box.set_valign(gtk4::Align::Center);
    lact_box.set_halign(gtk4::Align::End);
    let lact_s = gtk4::ToggleButton::builder().label("Saver").build();
    let lact_b = gtk4::ToggleButton::builder()
        .label("Balanced")
        .group(&lact_s)
        .build();
    let lact_p = gtk4::ToggleButton::builder()
        .label("Performance")
        .group(&lact_s)
        .build();
    lact_box.append(&lact_s);
    lact_box.append(&lact_b);
    lact_box.append(&lact_p);

    let cur_lact = get_current_lact_profile();
    if cur_lact == "Powersave" {
        lact_s.set_active(true);
    } else if cur_lact == "Performance" {
        lact_p.set_active(true);
    } else {
        lact_b.set_active(true);
    }

    lact_s.connect_toggled(|c| {
        if c.is_active() {
            Command::new("lact")
                .args(["cli", "profile", "set", "Powersave"])
                .spawn()
                .ok();
        }
    });
    lact_b.connect_toggled(|c| {
        if c.is_active() {
            Command::new("lact")
                .args(["cli", "profile", "set", "Balanced"])
                .spawn()
                .ok();
        }
    });
    lact_p.connect_toggled(|c| {
        if c.is_active() {
            Command::new("lact")
                .args(["cli", "profile", "set", "Performance"])
                .spawn()
                .ok();
        }
    });
    hw_grid.attach(&lbl_lact, 0, 2, 1, 1);
    hw_grid.attach(&lact_box, 1, 2, 1, 1);

    let lbl_fan = gtk4::Label::builder()
        .use_markup(true)
        .label("<b>Fan Control:</b>\n<span size='small' color='gray'>Aorus laptop fan speed</span>")
        .xalign(0.0)
        .yalign(0.5)
        .build();
    let fan_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    fan_box.add_css_class("linked");
    fan_box.set_valign(gtk4::Align::Center);
    fan_box.set_halign(gtk4::Align::End);
    let fan_q = gtk4::ToggleButton::builder().label("Quiet").build();
    let fan_a = gtk4::ToggleButton::builder()
        .label("Auto")
        .group(&fan_q)
        .build();
    let fan_m = gtk4::ToggleButton::builder()
        .label("Max")
        .group(&fan_q)
        .build();
    fan_box.append(&fan_q);
    fan_box.append(&fan_a);
    fan_box.append(&fan_m);

    match get_fan_mode().as_str() {
        "max" => fan_m.set_active(true),
        "quiet" => fan_q.set_active(true),
        _ => fan_a.set_active(true),
    }

    fan_q.connect_toggled(|c| {
        if c.is_active() {
            Command::new("/run/wrappers/bin/fan-control")
                .arg("quiet")
                .spawn()
                .ok();
        }
    });
    fan_a.connect_toggled(|c| {
        if c.is_active() {
            Command::new("/run/wrappers/bin/fan-control")
                .arg("auto")
                .spawn()
                .ok();
        }
    });
    fan_m.connect_toggled(|c| {
        if c.is_active() {
            Command::new("/run/wrappers/bin/fan-control")
                .arg("max")
                .spawn()
                .ok();
        }
    });
    hw_grid.attach(&lbl_fan, 0, 3, 1, 1);
    hw_grid.attach(&fan_box, 1, 3, 1, 1);

    vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let lbl_sw_title = gtk4::Label::builder()
        .label("<span weight='bold'>UI &amp; Desktop Powersave</span>")
        .use_markup(true)
        .xalign(0.0)
        .build();
    lbl_sw_title.set_margin_top(5);
    vbox.append(&lbl_sw_title);

    let sw_grid = gtk4::Grid::new();
    sw_grid.set_column_spacing(20);
    sw_grid.set_row_spacing(15);
    sw_grid.set_margin_top(5);
    sw_grid.set_margin_bottom(5);
    vbox.append(&sw_grid);

    let sw_anim = gtk4::Switch::builder()
        .valign(gtk4::Align::Center)
        .halign(gtk4::Align::End)
        .active(get_hypr_animations())
        .build();
    let sw_fps = gtk4::Switch::builder()
        .valign(gtk4::Align::Center)
        .halign(gtk4::Align::End)
        .active(is_edp_60hz())
        .build();
    let sw_rep = gtk4::Switch::builder()
        .valign(gtk4::Align::Center)
        .halign(gtk4::Align::End)
        .active(is_replays_running())
        .build();
    let sw_eco = gtk4::Switch::builder()
        .valign(gtk4::Align::Center)
        .halign(gtk4::Align::End)
        .active(!sw_anim.is_active() && sw_fps.is_active() && !sw_rep.is_active())
        .build();

    let updating_ui = Rc::new(Cell::new(false));

    let update_eco = {
        let sw_eco = sw_eco.clone();
        let sw_anim = sw_anim.clone();
        let sw_fps = sw_fps.clone();
        let sw_rep = sw_rep.clone();
        move || {
            let is_eco = !sw_anim.is_active() && sw_fps.is_active() && !sw_rep.is_active();
            if sw_eco.is_active() != is_eco {
                sw_eco.set_active(is_eco);
            }
        }
    };

    sw_eco.connect_state_set({
        let u = updating_ui.clone();
        let sw_anim = sw_anim.clone();
        let sw_fps = sw_fps.clone();
        let sw_rep = sw_rep.clone();
        move |_, state| {
            if u.get() {
                return glib::Propagation::Proceed;
            }
            u.set(true);
            sw_anim.set_active(!state);
            sw_fps.set_active(state);
            sw_rep.set_active(!state);
            u.set(false);
            apply_software_powersave(!state, state, !state);
            glib::Propagation::Proceed
        }
    });

    sw_anim.connect_state_set({
        let u = updating_ui.clone();
        let upd_eco = update_eco.clone();
        let s_fps = sw_fps.clone();
        let s_rep = sw_rep.clone();
        move |_, state| {
            if u.get() {
                return glib::Propagation::Proceed;
            }
            apply_software_powersave(state, s_fps.is_active(), s_rep.is_active());
            u.set(true);
            upd_eco();
            u.set(false);
            glib::Propagation::Proceed
        }
    });

    sw_fps.connect_state_set({
        let u = updating_ui.clone();
        let upd_eco = update_eco.clone();
        let s_anim = sw_anim.clone();
        let s_rep = sw_rep.clone();
        move |_, state| {
            if u.get() {
                return glib::Propagation::Proceed;
            }
            apply_software_powersave(s_anim.is_active(), state, s_rep.is_active());
            u.set(true);
            upd_eco();
            u.set(false);
            glib::Propagation::Proceed
        }
    });

    sw_rep.connect_state_set({
        let u = updating_ui.clone();
        let upd_eco = update_eco.clone();
        let s_anim = sw_anim.clone();
        let s_fps = sw_fps.clone();
        move |_, state| {
            if u.get() {
                return glib::Propagation::Proceed;
            }
            apply_software_powersave(s_anim.is_active(), s_fps.is_active(), state);
            u.set(true);
            upd_eco();
            u.set(false);
            glib::Propagation::Proceed
        }
    });

    sw_grid.attach(&gtk4::Label::builder().use_markup(true).label("<b>Batch Software Powersaving:</b>\n<span size='small' color='gray'>Toggles animations, 60Hz rate, &amp; replays</span>").xalign(0.0).yalign(0.5).build(), 0, 0, 1, 1);
    sw_grid.attach(&sw_eco, 1, 0, 1, 1);
    sw_grid.attach(
        &gtk4::Separator::new(gtk4::Orientation::Horizontal),
        0,
        1,
        2,
        1,
    );
    sw_grid.attach(&gtk4::Label::builder().use_markup(true).label("<b>Hyprland Animations:</b>\n<span size='small' color='gray'>Rendering animations &amp; borders</span>").xalign(0.0).yalign(0.5).build(), 0, 2, 1, 1);
    sw_grid.attach(&sw_anim, 1, 2, 1, 1);
    sw_grid.attach(&gtk4::Label::builder().use_markup(true).label("<b>Limit Screen to 60Hz:</b>\n<span size='small' color='gray'>eDP-1 display resolution refresh rate</span>").xalign(0.0).yalign(0.5).build(), 0, 3, 1, 1);
    sw_grid.attach(&sw_fps, 1, 3, 1, 1);
    sw_grid.attach(&gtk4::Label::builder().use_markup(true).label("<b>Enable Replays Service:</b>\n<span size='small' color='gray'>systemctl --user service status</span>").xalign(0.0).yalign(0.5).build(), 0, 4, 1, 1);
    sw_grid.attach(&sw_rep, 1, 4, 1, 1);

    let bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    bbox.set_halign(gtk4::Align::End);
    let btn_close = gtk4::Button::with_label("Close");
    btn_close.set_focusable(false);
    let w_close = win.clone();
    btn_close.connect_clicked(move |_| w_close.close());
    bbox.append(&btn_close);
    vbox.append(&bbox);

    let ml2 = main_loop.clone();
    win.connect_close_request(move |_| {
        ml2.quit();
        glib::Propagation::Proceed
    });
    win.present();

    main_loop.run();
}
