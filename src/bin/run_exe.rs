use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use system_ui_helpers::*;

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
            if response == gtk4::ResponseType::Accept {
                if let Some(f) = d.file() {
                    let path = f.path().unwrap().to_string_lossy().to_string();
                    d.destroy();
                    launch_window(&path, ml.clone());
                    return;
                }
            }
            d.destroy();
            ml.quit();
        });
        dlg.show();
        main_loop.run();
        return;
    }

    launch_window(&args[1], main_loop.clone());
    main_loop.run();
}

fn launch_window(filepath: &str, main_loop: glib::MainLoop) {
    let win = gtk4::Window::builder()
        .title("UMU Launcher")
        .default_width(560)
        .resizable(false)
        .build();

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
            if response == gtk4::ResponseType::Accept {
                if let Some(f) = d.file() {
                    ec.set_text(&f.path().unwrap().to_string_lossy());
                }
            }
            d.destroy();
        });
        dlg.show();
    });

    let proton_versions = load_proton_versions();
    let default_proton = get_default_proton_name(&proton_versions);
    let cur_proton = env::var("UMU_PROTON_TYPE").unwrap_or_else(|_| default_proton.clone());

    let cmb_proton = gtk4::ComboBoxText::new();
    for v in &proton_versions {
        cmb_proton.append(Some(&v.name), &v.name);
    }
    if !cmb_proton.set_active_id(Some(&cur_proton)) {
        cmb_proton.set_active(Some(0));
    }

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

    let lock_signals = Rc::new(RefCell::new(false));
    let l1 = lock_signals.clone();
    let w_chk1 = chk_wayland.clone();
    chk_overlay.connect_toggled(move |btn| {
        if *l1.borrow() {
            return;
        }
        *l1.borrow_mut() = true;
        if btn.is_active() {
            w_chk1.set_active(false);
            w_chk1.set_sensitive(false);
        } else {
            w_chk1.set_sensitive(true);
        }
        *l1.borrow_mut() = false;
    });

    let l2 = lock_signals.clone();
    let o_chk1 = chk_overlay.clone();
    chk_wayland.connect_toggled(move |btn| {
        if *l2.borrow() {
            return;
        }
        *l2.borrow_mut() = true;
        if btn.is_active() {
            o_chk1.set_active(false);
            o_chk1.set_sensitive(false);
        } else {
            o_chk1.set_sensitive(true);
        }
        *l2.borrow_mut() = false;
    });

    if chk_overlay.is_active() {
        chk_wayland.set_active(false);
        chk_wayland.set_sensitive(false);
    } else if chk_wayland.is_active() {
        chk_overlay.set_active(false);
        chk_overlay.set_sensitive(false);
    }

    let ent_prefix = gtk4::Entry::new();
    ent_prefix.set_text(&env::var("UMU_PREFIX_NAME").unwrap_or_else(|_| "default".into()));

    let cmb_gpu = gtk4::ComboBoxText::new();
    let gpu_opts = ["Автоматически", "AMD", "Nvidia", "Intel"];
    for opt in gpu_opts {
        cmb_gpu.append(Some(opt), opt);
    }
    let cur_gpu = env::var("UMU_GPU_SELECT").unwrap_or_else(|_| "Автоматически".into());
    if !cmb_gpu.set_active_id(Some(&cur_gpu)) {
        cmb_gpu.set_active(Some(0));
    }

    let mut orig_args = String::new();
    if filepath.to_lowercase().ends_with(".lnk") {
        if let Ok(out) = Command::new("exiftool")
            .args(["-s3", "-CommandLineArguments", filepath])
            .output()
        {
            let args_out = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !args_out.is_empty() && args_out != "-" {
                orig_args = args_out;
            }
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

    let add_row = |g: &gtk4::Grid, row: i32, title: &str, widget: &gtk4::Widget| {
        let lbl = gtk4::Label::builder()
            .label(title)
            .xalign(0.0)
            .yalign(0.5)
            .build();
        g.attach(&lbl, 0, row, 1, 1);
        g.attach(widget, 1, row, 1, 1);
    };

    add_row(&grid, 0, "Файл запуска", exe_hbox.upcast_ref());
    add_row(&grid, 1, "Версия Proton", cmb_proton.upcast_ref());
    add_row(&grid, 2, "GameMode", chk_gamemode.upcast_ref());
    add_row(&grid, 3, "MangoHud", chk_mangohud.upcast_ref());
    add_row(&grid, 4, "Wayland", chk_wayland.upcast_ref());
    add_row(&grid, 5, "Интегр. Steam", chk_steam.upcast_ref());
    add_row(&grid, 6, "Оверлей Steam", chk_overlay.upcast_ref());
    add_row(&grid, 7, "Через VPN", chk_vpn.upcast_ref());
    add_row(&grid, 8, "Префикс (в ~/.umu/)", ent_prefix.upcast_ref());
    add_row(&grid, 9, "Видеокарта", cmb_gpu.upcast_ref());
    add_row(&grid, 10, "Аргументы / Env", ent_args.upcast_ref());
    add_row(&grid, 11, "Game ID / App ID", gameid_hbox.upcast_ref());

    let desktop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    desktop_box.set_visible(false);
    vbox.append(&desktop_box);

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
    add_row(&c_grid, 0, "Название ярлыка", ent_name.upcast_ref());

    let img_preview = gtk4::Image::new();
    img_preview.set_pixel_size(48);
    img_preview.set_valign(gtk4::Align::Center);
    let btn_zoom = gtk4::Button::with_label("Увеличить");

    let ip_cb2 = img_preview.clone();
    let fc_icon = FileChooserButton::new("Выберите иконку", &win, move |path| {
        set_image_from_path_or_theme(&ip_cb2, &path, 48);
    });

    let default_extracted = extract_default_icon_for_exe(filepath, &ent_prefix.text());
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
    add_row(&c_grid, 1, "Иконка (файл)", icon_hbox.upcast_ref());

    let preview_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    preview_hbox.append(&img_preview);
    preview_hbox.append(&btn_zoom);
    add_row(&c_grid, 2, "Предпросмотр иконки", preview_hbox.upcast_ref());

    let fc_r = fc_icon.clone();
    let ip_reset = img_preview.clone();
    let ee_r = exe_entry.clone();
    let ep_r = ent_prefix.clone();
    btn_icon_reset.connect_clicked(move |_| {
        let cur_exe = ee_r.text().to_string();
        let target_icon = if let Some(extracted) = extract_default_icon_for_exe(&cur_exe, &ep_r.text()) {
            extracted
        } else {
            "wine".to_string()
        };
        fc_r.set_filename(&target_icon);
        set_image_from_path_or_theme(&ip_reset, &target_icon, 48);
    });

    let w_zoom = win.clone();
    let fc_z = fc_icon.clone();
    btn_zoom.connect_clicked(move |_| {
        let path = fc_z.get_filename();
        open_zoom_preview(&w_zoom, &path);
    });

    let w_st_game = win.clone();
    let gid_st = ent_gameid.clone();
    let name_st = ent_name.clone();
    let fc_s1 = fc_icon.clone();
    let ip_s1 = img_preview.clone();
    let def_n = default_name.clone();
    btn_steam_search.connect_clicked(move |_| {
        let g = gid_st.clone();
        let n = name_st.clone();
        let fc = fc_s1.clone();
        let ip = ip_s1.clone();
        let dn = def_n.clone();
        open_steam_search_dialog(&w_st_game, move |game_name, appid, icon_path| {
            g.set_text(&appid);
            let cur_n = n.text().to_string();
            if cur_n.is_empty() || cur_n == dn {
                n.set_text(&game_name);
            }
            if let Some(ref ico) = icon_path {
                fc.set_filename(ico);
                set_image_from_path_or_theme(&ip, ico, 48);
            }
        });
    });

    let w_st_icon = win.clone();
    let name_st2 = ent_name.clone();
    let fc_s2 = fc_icon.clone();
    let ip_s2 = img_preview.clone();
    let def_n2 = default_name.clone();
    btn_icon_search.connect_clicked(move |_| {
        let n = name_st2.clone();
        let fc = fc_s2.clone();
        let ip = ip_s2.clone();
        let dn = def_n2.clone();
        open_steam_search_dialog(&w_st_icon, move |game_name, _appid, icon_path| {
            let cur_n = n.text().to_string();
            if cur_n.is_empty() || cur_n == dn {
                n.set_text(&game_name);
            }
            if let Some(ref ico) = icon_path {
                fc.set_filename(ico);
                set_image_from_path_or_theme(&ip, ico, 48);
            }
        });
    });

    let launcher_bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    launcher_bbox.set_halign(gtk4::Align::End);
    let btn_run = gtk4::Button::with_label("Запустить");
    let btn_create_prompt = gtk4::Button::with_label("Создать .desktop");
    let btn_cancel = gtk4::Button::with_label("Отмена");
    launcher_bbox.append(&btn_run);
    launcher_bbox.append(&btn_create_prompt);
    launcher_bbox.append(&btn_cancel);
    vbox.append(&launcher_bbox);

    let creator_bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    creator_bbox.set_halign(gtk4::Align::End);
    creator_bbox.set_visible(false);
    let btn_save = gtk4::Button::with_label("Сохранить ярлык");
    let btn_back = gtk4::Button::with_label("Назад");
    creator_bbox.append(&btn_save);
    creator_bbox.append(&btn_back);
    vbox.append(&creator_bbox);

    let db_p = desktop_box.clone();
    let lb_p = launcher_bbox.clone();
    let cb_p = creator_bbox.clone();
    btn_create_prompt.connect_clicked(move |_| {
        lb_p.set_visible(false);
        db_p.set_visible(true);
        cb_p.set_visible(true);
    });

    let db_b = desktop_box.clone();
    let lb_b = launcher_bbox.clone();
    let cb_b = creator_bbox.clone();
    btn_back.connect_clicked(move |_| {
        db_b.set_visible(false);
        cb_b.set_visible(false);
        lb_b.set_visible(true);
    });

    let ml_c = main_loop.clone();
    btn_cancel.connect_clicked(move |_| ml_c.quit());

    let ee_run = exe_entry.clone();
    let ea_run = ent_args.clone();
    let cp_run = cmb_proton.clone();
    let cgm_run = chk_gamemode.clone();
    let cmh_run = chk_mangohud.clone();
    let cwl_run = chk_wayland.clone();
    let cst_run = chk_steam.clone();
    let cso_run = chk_overlay.clone();
    let cvp_run = chk_vpn.clone();
    let ep_run = ent_prefix.clone();
    let cg_run = cmb_gpu.clone();
    let eg_run = ent_gameid.clone();
    let ml_run = main_loop.clone();

    btn_run.connect_clicked(move |_| {
        let target_exe = ee_run.text().to_string();
        let raw_args = ea_run.text().to_string();
        let proton_type = cp_run.active_text().unwrap_or_default().to_string();
        let gpu_select = cg_run.active_text().unwrap_or_default().to_string();

        let mut envs: HashMap<String, String> = HashMap::new();
        envs.insert("UMU_PROTON_TYPE".into(), proton_type);
        envs.insert(
            "USE_GAMEMODE".into(),
            if cgm_run.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "USE_MANGOHUD".into(),
            if cmh_run.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "PROTON_ENABLE_WAYLAND".into(),
            if cwl_run.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "USE_STEAM_INTEGRATION".into(),
            if cst_run.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "USE_STEAM_OVERLAY".into(),
            if cso_run.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "USE_VPN".into(),
            if cvp_run.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert("UMU_PREFIX_NAME".into(), ep_run.text().to_string());
        envs.insert("UMU_GPU_SELECT".into(), gpu_select);
        envs.insert("GAMEID".into(), eg_run.text().to_string());

        let cmd_str = if raw_args.contains("%command%") {
            let parts: Vec<&str> = raw_args.splitn(2, "%command%").collect();
            format!(
                "{} umu-run-wrapper \"{}\" {}",
                parts[0].trim(),
                target_exe,
                parts.get(1).unwrap_or(&"").trim()
            )
        } else {
            format!("umu-run-wrapper \"{}\" {}", target_exe, raw_args.trim())
        };

        Command::new("sh")
            .arg("-c")
            .arg(format!("{}; scan-umu-for-lnk", cmd_str))
            .envs(&envs)
            .spawn()
            .ok();

        ml_run.quit();
    });

    let ee_save = exe_entry.clone();
    let ea_save = ent_args.clone();
    let cp_save = cmb_proton.clone();
    let cgm_save = chk_gamemode.clone();
    let cmh_save = chk_mangohud.clone();
    let cwl_save = chk_wayland.clone();
    let cst_save = chk_steam.clone();
    let cso_save = chk_overlay.clone();
    let cvp_save = chk_vpn.clone();
    let ep_save = ent_prefix.clone();
    let cg_save = cmb_gpu.clone();
    let eg_save = ent_gameid.clone();
    let en_save = ent_name.clone();
    let fc_save = fc_icon.clone();
    let ml_save = main_loop.clone();

    btn_save.connect_clicked(move |_| {
        let target_exe = ee_save.text().to_string();
        let name = en_save.text().to_string();
        let sel_icon = fc_save.get_filename();
        let cache_prefix = get_cached_icons_dir().to_string_lossy().to_string();
        let icon = if sel_icon.starts_with(&cache_prefix) {
            sel_icon
        } else {
            persist_icon(&sel_icon)
        };
        let args = ea_save.text().to_string();
        let proton_type = cp_save.active_text().unwrap_or_default().to_string();
        let gpu_select = cg_save.active_text().unwrap_or_default().to_string();

        let mut envs: HashMap<String, String> = HashMap::new();
        envs.insert("UMU_PROTON_TYPE".into(), proton_type);
        envs.insert(
            "USE_GAMEMODE".into(),
            if cgm_save.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "USE_MANGOHUD".into(),
            if cmh_save.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "PROTON_ENABLE_WAYLAND".into(),
            if cwl_save.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "USE_STEAM_INTEGRATION".into(),
            if cst_save.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "USE_STEAM_OVERLAY".into(),
            if cso_save.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert(
            "USE_VPN".into(),
            if cvp_save.is_active() { "1" } else { "0" }.into(),
        );
        envs.insert("UMU_PREFIX_NAME".into(), ep_save.text().to_string());
        envs.insert("UMU_GPU_SELECT".into(), gpu_select);
        envs.insert("GAMEID".into(), eg_save.text().to_string());

        let mut actual_exe = target_exe.clone();
        let mut lnk_arg = String::new();

        if target_exe.to_lowercase().ends_with(".lnk") {
            lnk_arg = target_exe.clone();
            if let Ok(out) = Command::new("exiftool")
                .args(["-s3", "-LocalBasePath", &target_exe])
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
                    let pfx_dir = PathBuf::from(home).join(".umu").join(ep_save.text());
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

        Command::new("create-desktop-with-umu")
            .arg(&actual_exe)
            .arg(&lnk_arg)
            .arg(&args)
            .arg(&name)
            .arg(&icon)
            .envs(&envs)
            .output()
            .ok();

        ml_save.quit();
    });

    let ml_win = main_loop.clone();
    win.connect_close_request(move |_| {
        ml_win.quit();
        glib::Propagation::Proceed
    });
    win.present();
}
