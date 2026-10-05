use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use system_ui_helpers::{
    FileChooserButton, PathsListWidget, apply_clean_theme, extract_default_icon_for_exe,
    get_cached_icons_dir, get_default_proton_name, load_proton_versions, open_steam_search_dialog,
    open_zoom_preview, persist_icon, set_image_from_path_or_theme,
};

#[derive(Clone)]
struct LauncherToggles {
    gamemode: gtk4::CheckButton,
    mangohud: gtk4::CheckButton,
    wayland: gtk4::CheckButton,
    steam: gtk4::CheckButton,
    overlay: gtk4::CheckButton,
    vpn: gtk4::CheckButton,
    sandbox: gtk4::CheckButton,
    gamepad: gtk4::CheckButton,
    network: gtk4::CheckButton,
    steam_ports: gtk4::CheckButton,
}

#[derive(Clone)]
struct LauncherWidgets {
    exe_entry: gtk4::Entry,
    cmb_proton: gtk4::ComboBoxText,
    toggles: LauncherToggles,
    ent_prefix: gtk4::Entry,
    cmb_gpu: gtk4::ComboBoxText,
    ent_args: gtk4::Entry,
    ent_gameid: gtk4::Entry,
    paths_widget: PathsListWidget,
}

fn build_launcher_toggles() -> LauncherToggles {
    let chk_gamemode = gtk4::CheckButton::new();
    chk_gamemode.set_active(env::var("USE_GAMEMODE").unwrap_or_else(|_| "1".into()) != "0");

    let chk_mangohud = gtk4::CheckButton::new();
    chk_mangohud.set_active(env::var("USE_MANGOHUD").unwrap_or_else(|_| "1".into()) != "0");

    let chk_wayland = gtk4::CheckButton::new();
    chk_wayland.set_active(env::var("PROTON_ENABLE_WAYLAND").unwrap_or_else(|_| "1".into()) != "0");

    let chk_steam = gtk4::CheckButton::new();
    chk_steam.set_active(env::var("USE_STEAM_INTEGRATION").unwrap_or_else(|_| "0".into()) == "1");

    let chk_overlay = gtk4::CheckButton::new();
    chk_overlay.set_active(env::var("USE_STEAM_OVERLAY").unwrap_or_else(|_| "0".into()) == "1");

    let chk_vpn = gtk4::CheckButton::new();
    chk_vpn.set_active(env::var("USE_VPN").unwrap_or_else(|_| "0".into()) == "1");

    let chk_sandbox = gtk4::CheckButton::new();
    chk_sandbox.set_active(env::var("USE_SANDBOX").unwrap_or_else(|_| "1".into()) != "0");

    let chk_gamepad = gtk4::CheckButton::new();
    chk_gamepad.set_active(env::var("USE_GAMEPAD").unwrap_or_else(|_| "1".into()) != "0");

    let chk_network = gtk4::CheckButton::new();
    chk_network.set_active(env::var("USE_NETWORK").unwrap_or_else(|_| "1".into()) != "0");

    let chk_steam_ports = gtk4::CheckButton::new();
    let default_sp = env::var("USE_STEAM_PORTS")
        .unwrap_or_else(|_| env::var("USE_STEAM_INTEGRATION").unwrap_or_else(|_| "0".into()));
    chk_steam_ports.set_active(default_sp == "1");

    let lock_signals = Rc::new(RefCell::new(false));
    let lock_ov = Rc::clone(&lock_signals);
    let wayland_btn = chk_wayland.clone();
    chk_overlay.connect_toggled(move |btn| {
        if *lock_ov.borrow() {
            return;
        }
        *lock_ov.borrow_mut() = true;
        if btn.is_active() {
            wayland_btn.set_active(false);
            wayland_btn.set_sensitive(false);
        } else {
            wayland_btn.set_sensitive(true);
        }
        *lock_ov.borrow_mut() = false;
    });

    let lock_wl = lock_signals;
    let overlay_btn = chk_overlay.clone();
    chk_wayland.connect_toggled(move |btn| {
        if *lock_wl.borrow() {
            return;
        }
        *lock_wl.borrow_mut() = true;
        if btn.is_active() {
            overlay_btn.set_active(false);
            overlay_btn.set_sensitive(false);
        } else {
            overlay_btn.set_sensitive(true);
        }
        *lock_wl.borrow_mut() = false;
    });

    if chk_overlay.is_active() {
        chk_wayland.set_active(false);
        chk_wayland.set_sensitive(false);
    } else if chk_wayland.is_active() {
        chk_overlay.set_active(false);
        chk_overlay.set_sensitive(false);
    }

    LauncherToggles {
        gamemode: chk_gamemode,
        mangohud: chk_mangohud,
        wayland: chk_wayland,
        steam: chk_steam,
        overlay: chk_overlay,
        vpn: chk_vpn,
        sandbox: chk_sandbox,
        gamepad: chk_gamepad,
        network: chk_network,
        steam_ports: chk_steam_ports,
    }
}

fn append_toggle_envs(envs: &mut HashMap<String, String>, toggles: &LauncherToggles) {
    let bool_str = |b: bool| if b { "1" } else { "0" };
    envs.insert(
        "USE_GAMEMODE".into(),
        bool_str(toggles.gamemode.is_active()).into(),
    );
    envs.insert(
        "USE_MANGOHUD".into(),
        bool_str(toggles.mangohud.is_active()).into(),
    );
    envs.insert(
        "PROTON_ENABLE_WAYLAND".into(),
        bool_str(toggles.wayland.is_active()).into(),
    );
    envs.insert(
        "USE_STEAM_INTEGRATION".into(),
        bool_str(toggles.steam.is_active()).into(),
    );
    envs.insert(
        "USE_STEAM_OVERLAY".into(),
        bool_str(toggles.overlay.is_active()).into(),
    );
    envs.insert("USE_VPN".into(), bool_str(toggles.vpn.is_active()).into());
    envs.insert(
        "USE_SANDBOX".into(),
        bool_str(toggles.sandbox.is_active()).into(),
    );
    envs.insert(
        "USE_GAMEPAD".into(),
        bool_str(toggles.gamepad.is_active()).into(),
    );
    envs.insert(
        "USE_NETWORK".into(),
        bool_str(toggles.network.is_active()).into(),
    );
    envs.insert(
        "USE_STEAM_PORTS".into(),
        bool_str(toggles.steam_ports.is_active()).into(),
    );
}

fn build_umu_envs(widgets: &LauncherWidgets) -> HashMap<String, String> {
    let proton_type = widgets
        .cmb_proton
        .active_text()
        .unwrap_or_default()
        .to_string();
    let gpu_select = widgets
        .cmb_gpu
        .active_text()
        .unwrap_or_default()
        .to_string();

    let mut envs: HashMap<String, String> = HashMap::new();
    envs.insert("UMU_PROTON_TYPE".into(), proton_type);
    envs.insert("UMU_GPU_SELECT".into(), gpu_select);
    envs.insert(
        "UMU_PREFIX_NAME".into(),
        widgets.ent_prefix.text().to_string(),
    );
    envs.insert("GAMEID".into(), widgets.ent_gameid.text().to_string());

    append_toggle_envs(&mut envs, &widgets.toggles);

    let extra_p = widgets.paths_widget.to_string_args();
    if !extra_p.is_empty() {
        envs.insert("UMU_EXTRA_PATHS".into(), extra_p);
    }
    envs
}

fn run_application(widgets: &LauncherWidgets) {
    let target_exe = widgets.exe_entry.text().to_string();
    let raw_args = widgets.ent_args.text().to_string();
    let envs = build_umu_envs(widgets);

    let cmd_str = if raw_args.contains("%command%") {
        let parts: Vec<&str> = raw_args.splitn(2, "%command%").collect();
        format!(
            "{} umu-run-wrapper \"{target_exe}\" {}",
            parts[0].trim(),
            parts.get(1).unwrap_or(&"").trim()
        )
    } else {
        format!("umu-run-wrapper \"{target_exe}\" {}", raw_args.trim())
    };

    Command::new("sh")
        .arg("-c")
        .arg(format!("{cmd_str}; scan-umu-for-lnk"))
        .envs(&envs)
        .spawn()
        .ok();
}

fn resolve_actual_exe_from_lnk(target_exe: &str, prefix_name: &str) -> (String, String) {
    let mut actual_exe = target_exe.to_string();
    let mut lnk_arg = String::new();

    if target_exe.to_lowercase().ends_with(".lnk") {
        lnk_arg.clone_from(&actual_exe);
        if let Ok(out) = Command::new("exiftool")
            .args(["-s3", "-LocalBasePath", target_exe])
            .output()
        {
            let win_p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !win_p.is_empty() && win_p != "-" {
                let rel_p = win_p.replace('\\', "/");
                let clean_p = if rel_p.len() >= 2 && rel_p.chars().nth(1) == Some(':') {
                    rel_p[2..].to_string()
                } else {
                    rel_p
                };
                let home = env::var("HOME").unwrap_or_else(|_| ".".into());
                let pfx_dir = PathBuf::from(home).join(".umu").join(prefix_name);
                let rel_clean = clean_p.trim_start_matches('/');
                let cand_upper = pfx_dir.join("upper/drive_c").join(rel_clean);
                let cand_legacy = pfx_dir.join("drive_c").join(rel_clean);
                let cand = if cand_upper.exists() {
                    cand_upper
                } else {
                    cand_legacy
                };
                if cand.exists() {
                    actual_exe = cand.to_string_lossy().to_string();
                }
            }
        }
    }
    (actual_exe, lnk_arg)
}

fn save_desktop_shortcut(widgets: &LauncherWidgets, name: &str, sel_icon: &str) {
    let target_exe = widgets.exe_entry.text().to_string();
    let cache_prefix = get_cached_icons_dir().to_string_lossy().to_string();
    let icon = if sel_icon.starts_with(&cache_prefix) {
        sel_icon.to_string()
    } else {
        persist_icon(sel_icon)
    };
    let args = widgets.ent_args.text().to_string();
    let envs = build_umu_envs(widgets);
    let (actual_exe, lnk_arg) =
        resolve_actual_exe_from_lnk(&target_exe, &widgets.ent_prefix.text());

    Command::new("create-desktop-with-umu")
        .arg(&actual_exe)
        .arg(&lnk_arg)
        .arg(&args)
        .arg(name)
        .arg(&icon)
        .envs(&envs)
        .output()
        .ok();
}

fn attach_launcher_rows(
    grid: &gtk4::Grid,
    widgets: &LauncherWidgets,
    exe_hbox: &gtk4::Box,
    gameid_hbox: &gtk4::Box,
) {
    let add_row = |g: &gtk4::Grid, row: i32, title: &str, widget: &gtk4::Widget| {
        let lbl = gtk4::Label::builder()
            .label(title)
            .xalign(0.0)
            .yalign(0.5)
            .build();
        g.attach(&lbl, 0, row, 1, 1);
        g.attach(widget, 1, row, 1, 1);
    };

    add_row(grid, 0, "Файл запуска", exe_hbox.upcast_ref());
    add_row(grid, 1, "Версия Proton", widgets.cmb_proton.upcast_ref());
    add_row(grid, 2, "GameMode", widgets.toggles.gamemode.upcast_ref());
    add_row(grid, 3, "MangoHud", widgets.toggles.mangohud.upcast_ref());
    add_row(grid, 4, "Wayland", widgets.toggles.wayland.upcast_ref());
    add_row(grid, 5, "Интегр. Steam", widgets.toggles.steam.upcast_ref());
    add_row(
        grid,
        6,
        "Оверлей Steam",
        widgets.toggles.overlay.upcast_ref(),
    );
    add_row(grid, 7, "Через VPN", widgets.toggles.vpn.upcast_ref());
    add_row(
        grid,
        8,
        "Префикс (в ~/.umu/)",
        widgets.ent_prefix.upcast_ref(),
    );
    add_row(grid, 9, "Видеокарта", widgets.cmb_gpu.upcast_ref());
    add_row(grid, 10, "Аргументы / Env", widgets.ent_args.upcast_ref());
    add_row(grid, 11, "Game ID / App ID", gameid_hbox.upcast_ref());
    add_row(grid, 12, "Песочница", widgets.toggles.sandbox.upcast_ref());
    add_row(
        grid,
        13,
        "Сеть песочницы",
        widgets.toggles.network.upcast_ref(),
    );
    add_row(
        grid,
        14,
        "Порты Steam",
        widgets.toggles.steam_ports.upcast_ref(),
    );
    add_row(grid, 15, "Геймпад", widgets.toggles.gamepad.upcast_ref());
}

struct CreatorButtons {
    zoom: gtk4::Button,
    icon_reset: gtk4::Button,
    icon_search: gtk4::Button,
}

fn connect_desktop_creator_buttons(
    win: &gtk4::Window,
    widgets: &LauncherWidgets,
    fc_icon: &FileChooserButton,
    img_preview: &gtk4::Image,
    ent_name: &gtk4::Entry,
    buttons: &CreatorButtons,
) {
    let fc_r = fc_icon.clone();
    let ip_reset = img_preview.clone();
    let input_exe_ent = widgets.exe_entry.clone();
    let input_pfx_ent = widgets.ent_prefix.clone();
    buttons.icon_reset.connect_clicked(move |_| {
        let cur_exe = input_exe_ent.text().to_string();
        let target_icon = extract_default_icon_for_exe(&cur_exe, &input_pfx_ent.text())
            .unwrap_or_else(|| "wine".to_string());
        fc_r.set_filename(&target_icon);
        set_image_from_path_or_theme(&ip_reset, &target_icon, 48);
    });

    let w_zm = win.clone();
    let fc_z = fc_icon.clone();
    buttons.zoom.connect_clicked(move |_| {
        open_zoom_preview(&w_zm, &fc_z.get_filename());
    });

    let w_steam = win.clone();
    let name_steam = ent_name.clone();
    let gid_steam = widgets.ent_gameid.clone();
    let fc_steam = fc_icon.clone();
    let img_steam = img_preview.clone();
    buttons.icon_search.connect_clicked(move |_| {
        let name_ent = name_steam.clone();
        let gid_ent = gid_steam.clone();
        let fc = fc_steam.clone();
        let img = img_steam.clone();
        open_steam_search_dialog(&w_steam, move |game_name, appid, icon_path| {
            gid_ent.set_text(&appid);
            name_ent.set_text(&game_name);
            if let Some(ref ico) = icon_path {
                fc.set_filename(ico);
                set_image_from_path_or_theme(&img, ico, 48);
            }
        });
    });
}

fn build_desktop_creator_box(
    win: &gtk4::Window,
    filepath: &str,
    widgets: &LauncherWidgets,
) -> (gtk4::Box, gtk4::Entry, FileChooserButton) {
    let desktop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    desktop_box.set_visible(false);

    desktop_box.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    let c_grid = gtk4::Grid::new();
    c_grid.set_column_spacing(15);
    c_grid.set_row_spacing(10);
    desktop_box.append(&c_grid);

    let default_name = if filepath.to_lowercase().ends_with(".lnk") {
        Path::new(filepath)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .replace(".lnk", "")
    } else if filepath.to_lowercase().ends_with(".exe") {
        Path::new(filepath)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .replace(".exe", "")
    } else {
        Path::new(filepath)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    };

    let ent_name = gtk4::Entry::new();
    ent_name.set_text(&default_name);
    ent_name.set_hexpand(true);

    let img_preview = gtk4::Image::new();
    img_preview.set_pixel_size(48);
    img_preview.set_valign(gtk4::Align::Center);
    let btn_zoom = gtk4::Button::with_label("Увеличить");

    let ip_cb2 = img_preview.clone();
    let fc_icon = FileChooserButton::new("Выберите иконку", win, move |path| {
        set_image_from_path_or_theme(&ip_cb2, &path, 48);
    });

    let default_extracted = extract_default_icon_for_exe(filepath, &widgets.ent_prefix.text());
    if let Some(ref ico) = default_extracted {
        fc_icon.set_filename(ico);
        set_image_from_path_or_theme(&img_preview, ico, 48);
    } else {
        fc_icon.set_filename("wine");
        set_image_from_path_or_theme(&img_preview, "wine", 48);
    }

    let btn_icon_search = gtk4::Button::with_label("Поиск в Steam");
    let btn_icon_reset = gtk4::Button::with_label("Сбросить");

    let icon_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    icon_hbox.append(fc_icon.widget());
    icon_hbox.append(&btn_icon_search);
    icon_hbox.append(&btn_icon_reset);

    let add_row = |g: &gtk4::Grid, row: i32, title: &str, widget: &gtk4::Widget| {
        let lbl = gtk4::Label::builder()
            .label(title)
            .xalign(0.0)
            .yalign(0.5)
            .build();
        g.attach(&lbl, 0, row, 1, 1);
        g.attach(widget, 1, row, 1, 1);
    };

    add_row(&c_grid, 0, "Название ярлыка", ent_name.upcast_ref());
    add_row(&c_grid, 1, "Иконка (файл)", icon_hbox.upcast_ref());

    let preview_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    preview_hbox.append(&img_preview);
    preview_hbox.append(&btn_zoom);
    add_row(&c_grid, 2, "Предпросмотр иконки", preview_hbox.upcast_ref());

    let buttons = CreatorButtons {
        zoom: btn_zoom,
        icon_reset: btn_icon_reset,
        icon_search: btn_icon_search,
    };

    connect_desktop_creator_buttons(win, widgets, &fc_icon, &img_preview, &ent_name, &buttons);

    (desktop_box, ent_name, fc_icon)
}

fn build_launcher_combos(default_proton: &str) -> (gtk4::ComboBoxText, gtk4::ComboBoxText) {
    let cur_proton = env::var("UMU_PROTON_TYPE").unwrap_or_else(|_| default_proton.to_string());
    let proton_versions = load_proton_versions();
    let cmb_proton = gtk4::ComboBoxText::new();
    for v in &proton_versions {
        cmb_proton.append(Some(&v.name), &v.name);
    }
    if !cmb_proton.set_active_id(Some(&cur_proton)) {
        cmb_proton.set_active(Some(0));
    }

    let cmb_gpu = gtk4::ComboBoxText::new();
    let gpu_opts = ["Автоматически", "AMD", "Nvidia", "Intel"];
    for opt in gpu_opts {
        cmb_gpu.append(Some(opt), opt);
    }
    let cur_gpu = env::var("UMU_GPU_SELECT").unwrap_or_else(|_| "Автоматически".into());
    if !cmb_gpu.set_active_id(Some(&cur_gpu)) {
        cmb_gpu.set_active(Some(0));
    }
    (cmb_proton, cmb_gpu)
}

fn build_launcher_action_boxes() -> (
    gtk4::Button,
    gtk4::Button,
    gtk4::Button,
    gtk4::Button,
    gtk4::Box,
    gtk4::Box,
) {
    let launcher_bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    launcher_bbox.set_halign(gtk4::Align::End);
    let btn_run = gtk4::Button::with_label("Запустить");
    btn_run.add_css_class("suggested-action");
    let btn_create_prompt = gtk4::Button::with_label("Создать ярлык...");
    let btn_cancel = gtk4::Button::with_label("Отмена");
    launcher_bbox.append(&btn_run);
    launcher_bbox.append(&btn_create_prompt);
    launcher_bbox.append(&btn_cancel);

    let creator_bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    creator_bbox.set_halign(gtk4::Align::End);
    creator_bbox.set_visible(false);
    let btn_save = gtk4::Button::with_label("Сохранить ярлык");
    btn_save.add_css_class("suggested-action");
    let btn_back = gtk4::Button::with_label("Назад");
    creator_bbox.append(&btn_save);
    creator_bbox.append(&btn_back);

    (
        btn_run,
        btn_create_prompt,
        btn_save,
        btn_back,
        launcher_bbox,
        creator_bbox,
    )
}

fn build_exe_browse_row(win: &gtk4::Window, filepath: &str) -> (gtk4::Box, gtk4::Entry) {
    let exe_entry = gtk4::Entry::new();
    exe_entry.set_text(filepath);
    exe_entry.set_hexpand(true);
    let btn_exe_browse = gtk4::Button::with_label("Обзор...");
    let exe_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    exe_hbox.append(&exe_entry);
    exe_hbox.append(&btn_exe_browse);

    let w_browse = win.clone();
    let ee_browse = exe_entry.clone();
    btn_exe_browse.connect_clicked(move |_| {
        let dlg = gtk4::FileChooserNative::builder()
            .title("Выберите исполняемый файл")
            .transient_for(&w_browse)
            .action(gtk4::FileChooserAction::Open)
            .build();
        let ec = ee_browse.clone();
        dlg.connect_response(move |d, response| {
            if response == gtk4::ResponseType::Accept
                && let Some(f) = d.file()
                && let Some(p) = f.path()
            {
                ec.set_text(&p.to_string_lossy());
            }
            d.destroy();
        });
        dlg.show();
    });
    (exe_hbox, exe_entry)
}

fn setup_launcher_ui(
    win: &gtk4::Window,
    filepath: &str,
) -> (
    LauncherWidgets,
    gtk4::Box,
    gtk4::Entry,
    FileChooserButton,
    gtk4::Button,
    gtk4::Button,
    gtk4::Button,
    gtk4::Button,
    gtk4::Box,
    gtk4::Box,
) {
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    vbox.set_margin_top(15);
    vbox.set_margin_bottom(15);
    vbox.set_margin_start(15);
    vbox.set_margin_end(15);
    win.set_child(Some(&vbox));

    let grid = gtk4::Grid::new();
    grid.set_column_spacing(15);
    grid.set_row_spacing(10);
    vbox.append(&grid);

    let (exe_hbox, exe_entry) = build_exe_browse_row(win, filepath);

    let proton_versions = load_proton_versions();
    let default_proton = get_default_proton_name(&proton_versions);
    let (cmb_proton, cmb_gpu) = build_launcher_combos(&default_proton);
    let toggles = build_launcher_toggles();

    let ent_prefix = gtk4::Entry::new();
    ent_prefix.set_text(&env::var("UMU_PREFIX_NAME").unwrap_or_else(|_| "default".into()));

    let mut orig_args = String::new();
    if filepath.to_lowercase().ends_with(".lnk")
        && let Ok(out) = Command::new("exiftool")
            .args(["-s3", "-CommandLineArguments", filepath])
            .output()
    {
        let args_out = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !args_out.is_empty() && args_out != "-" {
            orig_args = args_out;
        }
    }

    let ent_args = gtk4::Entry::new();
    ent_args.set_text(&orig_args);
    ent_args.set_placeholder_text(Some("VAR=1 %command% --dx11"));

    let ent_gameid = gtk4::Entry::new();
    ent_gameid.set_text(&env::var("GAMEID").unwrap_or_default());
    ent_gameid.set_placeholder_text(Some("Например: 292030 (AppID)"));
    ent_gameid.set_hexpand(true);

    let btn_steam_search = gtk4::Button::with_label("Поиск в Steam");
    let gameid_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    gameid_hbox.append(&ent_gameid);
    gameid_hbox.append(&btn_steam_search);

    let paths_widget = PathsListWidget::new(win);
    let has_extra = env::var("UMU_EXTRA_PATHS").is_ok_and(|extra_paths| {
        let trimmed = extra_paths.trim();
        if trimmed.is_empty() {
            false
        } else {
            paths_widget.load_from_string_args(trimmed);
            true
        }
    });
    let paths_expander = gtk4::Expander::builder()
        .label("Дополнительные каталоги (Монтирование)")
        .child(paths_widget.widget())
        .expanded(has_extra)
        .build();

    let widgets = LauncherWidgets {
        exe_entry,
        cmb_proton,
        toggles,
        ent_prefix,
        cmb_gpu,
        ent_args,
        ent_gameid,
        paths_widget,
    };

    attach_launcher_rows(&grid, &widgets, &exe_hbox, &gameid_hbox);
    vbox.append(&paths_expander);

    let (desktop_box, ent_name, fc_icon) = build_desktop_creator_box(win, filepath, &widgets);
    vbox.append(&desktop_box);

    let (btn_run, btn_create_prompt, btn_save, btn_back, launcher_bbox, creator_bbox) =
        build_launcher_action_boxes();
    vbox.append(&launcher_bbox);
    vbox.append(&creator_bbox);

    (
        widgets,
        desktop_box,
        ent_name,
        fc_icon,
        btn_run,
        btn_create_prompt,
        btn_save,
        btn_back,
        launcher_bbox,
        creator_bbox,
    )
}

fn launch_window(filepath: &str, main_loop: &glib::MainLoop) {
    let win = gtk4::Window::builder()
        .title("UMU Launcher")
        .default_width(560)
        .resizable(false)
        .build();

    let (
        widgets,
        desktop_box,
        ent_name,
        fc_icon,
        btn_run,
        btn_create_prompt,
        btn_save,
        btn_back,
        launcher_bbox,
        creator_bbox,
    ) = setup_launcher_ui(&win, filepath);

    let db_p = desktop_box.clone();
    let lb_p = launcher_bbox.clone();
    let cb_p = creator_bbox.clone();
    btn_create_prompt.connect_clicked(move |_| {
        lb_p.set_visible(false);
        db_p.set_visible(true);
        cb_p.set_visible(true);
    });

    let db_b = desktop_box;
    let lb_b = launcher_bbox;
    let cb_b = creator_bbox;
    btn_back.connect_clicked(move |_| {
        db_b.set_visible(false);
        cb_b.set_visible(false);
        lb_b.set_visible(true);
    });

    let ml_run = main_loop.clone();
    let w_run = widgets.clone();
    btn_run.connect_clicked(move |_| {
        run_application(&w_run);
        ml_run.quit();
    });

    let ml_save = main_loop.clone();
    btn_save.connect_clicked(move |_| {
        let name = ent_name.text().to_string();
        let sel_icon = fc_icon.get_filename();
        save_desktop_shortcut(&widgets, &name, &sel_icon);
        ml_save.quit();
    });

    let ml_win = main_loop.clone();
    win.connect_close_request(move |_| {
        ml_win.quit();
        glib::Propagation::Proceed
    });
    win.present();
}

fn main() {
    gtk4::init().expect("GTK4 init failed");
    apply_clean_theme();
    let main_loop = glib::MainLoop::new(None, false);

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        let dummy = gtk4::Window::new();
        let dlg = gtk4::FileChooserNative::builder()
            .title("Выберите файл")
            .transient_for(&dummy)
            .action(gtk4::FileChooserAction::Open)
            .build();

        let ml = main_loop.clone();
        dlg.connect_response(move |d, response| {
            if response == gtk4::ResponseType::Accept
                && let Some(f) = d.file()
                && let Some(p) = f.path()
            {
                let path = p.to_string_lossy().to_string();
                d.destroy();
                launch_window(&path, &ml);
                return;
            }
            d.destroy();
            ml.quit();
        });
        dlg.show();
        main_loop.run();
        return;
    }

    launch_window(&args[1], &main_loop);
    main_loop.run();
}
