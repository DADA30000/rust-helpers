use gtk4::prelude::*;
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use system_ui_helpers::*;

fn main() {
    gtk4::init().expect("GTK4 init failed");
    apply_clean_theme();
    let main_loop = glib::MainLoop::new(None, false);

    let win = gtk4::Window::builder()
        .title("Manage UMU Shortcuts")
        .default_width(750)
        .default_height(480)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_top(10);
    vbox.set_margin_bottom(10);
    vbox.set_margin_start(10);
    vbox.set_margin_end(10);
    win.set_child(Some(&vbox));

    let search_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    let search_lbl = gtk4::Label::new(Some("Поиск ярлыков"));
    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_hexpand(true);
    search_entry.set_can_focus(true);
    search_hbox.append(&search_lbl);
    search_hbox.append(&search_entry);
    vbox.append(&search_hbox);

    let listbox = gtk4::ListBox::new();
    listbox.set_selection_mode(gtk4::SelectionMode::Single);
    let scroll = gtk4::ScrolledWindow::builder()
        .child(&listbox)
        .vexpand(true)
        .build();
    vbox.append(&scroll);

    let populate = Rc::new(move |lb: &gtk4::ListBox| {
        while let Some(child) = lb.first_child() {
            lb.remove(&child);
        }
        let desk_dir = get_xdg_data_home().join("applications");
        if let Ok(entries) = fs::read_dir(desk_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("umu-") && name.ends_with(".desktop") {
                    let path = entry.path().to_string_lossy().to_string();
                    let dname = read_desktop_prop(&path, "Name");
                    let icon_path = read_desktop_prop(&path, "Icon");

                    let row = gtk4::ListBoxRow::new();
                    row.set_widget_name(&format!("{}|{}", dname, path));

                    let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 15);
                    row_box.set_margin_top(8);
                    row_box.set_margin_bottom(8);
                    row_box.set_margin_start(10);
                    row_box.set_margin_end(10);

                    let img = gtk4::Image::new();
                    set_image_from_path_or_theme(&img, &icon_path, 64);

                    let lbl = gtk4::Label::builder()
                        .label(&dname)
                        .xalign(0.0)
                        .hexpand(true)
                        .build();

                    row_box.append(&img);
                    row_box.append(&lbl);
                    row.set_child(Some(&row_box));
                    lb.append(&row);
                }
            }
        }
    });

    populate(&listbox);

    let s_clone = search_entry.clone();
    listbox.set_filter_func(move |row| {
        let query = s_clone.text().trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        let data = row.widget_name().to_lowercase();
        let (name, _) = data.split_once('|').unwrap_or((&data, ""));
        name.contains(&query)
    });

    let lb_filter = listbox.clone();
    search_entry.connect_search_changed(move |_| {
        lb_filter.invalidate_filter();
    });

    let bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    bbox.set_halign(gtk4::Align::End);
    let btn_delete = gtk4::Button::with_label("Удалить ярлык");
    let btn_close = gtk4::Button::with_label("Закрыть");

    btn_delete.set_focusable(false);
    btn_delete.set_can_focus(false);
    btn_close.set_focusable(false);
    btn_close.set_can_focus(false);

    bbox.append(&btn_delete);
    bbox.append(&btn_close);
    vbox.append(&bbox);

    let w_close = win.clone();
    btn_close.connect_clicked(move |_| w_close.close());

    let get_selected = {
        let lb = listbox.clone();
        move || -> Option<(String, String)> {
            if let Some(row) = lb.selected_row() {
                let data = row.widget_name().to_string();
                if let Some((name, path)) = data.split_once('|') {
                    return Some((name.to_string(), path.to_string()));
                }
            }
            None
        }
    };

    let w_edit = win.clone();
    let gs_edit = get_selected.clone();
    let pop_edit = populate.clone();
    let lb_edit = listbox.clone();

    listbox.connect_row_activated(move |_, _| {
        if let Some((_, desk_path)) = gs_edit() {
            open_edit_dialog(&w_edit, &desk_path, {
                let p = pop_edit.clone();
                let lb = lb_edit.clone();
                move || p(&lb)
            });
        }
    });

    let w_del = win.clone();
    let gs_del = get_selected.clone();
    let pop_del = populate.clone();
    let lb_del = listbox.clone();
    btn_delete.connect_clicked(move |_| {
        if let Some((name, desk_path)) = gs_del() {
            let dlg = gtk4::MessageDialog::builder()
                .transient_for(&w_del)
                .modal(true)
                .message_type(gtk4::MessageType::Question)
                .buttons(gtk4::ButtonsType::YesNo)
                .text(format!(
                    "Вы уверены, что хотите полностью удалить ярлык '{}'?",
                    name
                ))
                .build();

            let p_clone = pop_del.clone();
            let lb_c = lb_del.clone();
            dlg.connect_response(move |d, response| {
                if response == gtk4::ResponseType::Yes {
                    fs::remove_file(&desk_path).ok();
                    p_clone(&lb_c);
                }
                d.destroy();
            });
            dlg.present();
        }
    });

    let ml_win = main_loop.clone();
    win.connect_close_request(move |_| {
        ml_win.quit();
        glib::Propagation::Proceed
    });

    search_entry.grab_focus();
    win.present();
    main_loop.run();
}

fn open_edit_dialog(parent: &gtk4::Window, desktop_path: &str, on_saved: impl Fn() + 'static) {
    let dlg = gtk4::Window::builder()
        .title("Редактирование ярлыка")
        .default_width(580)
        .transient_for(parent)
        .modal(true)
        .resizable(false)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    vbox.set_margin_top(15);
    vbox.set_margin_bottom(15);
    vbox.set_margin_start(15);
    vbox.set_margin_end(15);
    dlg.set_child(Some(&vbox));

    let current_name = read_desktop_prop(desktop_path, "Name");
    let current_icon = read_desktop_prop(desktop_path, "Icon");
    let raw_args = read_desktop_prop(desktop_path, "X-UMU-Raw-Args");
    let actual_exe = read_desktop_prop(desktop_path, "X-UMU-Actual-Exe");
    let prefix_name = read_desktop_prop(desktop_path, "X-UMU-Prefix-Name");
    let gpu_select = read_desktop_prop(desktop_path, "X-UMU-GPU-Select");
    let exec_line = read_desktop_prop(desktop_path, "Exec");
    let proton_type = read_desktop_prop(desktop_path, "X-UMU-Proton-Type");
    let gameid = read_desktop_prop(desktop_path, "X-UMU-Game-ID");

    let title_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    let lbl_p = gtk4::Label::builder()
        .label("<b>Редактирование:</b>")
        .use_markup(true)
        .build();
    let lbl_f = gtk4::Label::builder()
        .label(&format!("<b>{}</b>", current_name))
        .use_markup(true)
        .build();
    title_hbox.append(&lbl_p);
    title_hbox.append(&lbl_f);
    vbox.append(&title_hbox);

    let grid = gtk4::Grid::new();
    grid.set_column_spacing(15);
    grid.set_row_spacing(10);
    vbox.append(&grid);

    let exe_entry = gtk4::Entry::new();
    exe_entry.set_text(&actual_exe);
    exe_entry.set_hexpand(true);
    let btn_exe_browse = gtk4::Button::with_label("Обзор...");
    let exe_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    exe_hbox.append(&exe_entry);
    exe_hbox.append(&btn_exe_browse);

    let w_exe = dlg.clone();
    let ee_c = exe_entry.clone();
    btn_exe_browse.connect_clicked(move |_| {
        let d = gtk4::FileChooserNative::builder()
            .title("Выберите исполняемый файл (.exe)")
            .transient_for(&w_exe)
            .action(gtk4::FileChooserAction::Open)
            .build();
        let ec = ee_c.clone();
        d.connect_response(move |dialog, res| {
            if res == gtk4::ResponseType::Accept {
                if let Some(f) = dialog.file() {
                    ec.set_text(&f.path().unwrap().to_string_lossy());
                }
            }
            dialog.destroy();
        });
        d.show();
    });

    let proton_versions = load_proton_versions();
    let cmb_proton = gtk4::ComboBoxText::new();
    for v in &proton_versions {
        cmb_proton.append(Some(&v.name), &v.name);
    }
    if !cmb_proton.set_active_id(Some(&proton_type)) {
        cmb_proton.set_active(Some(0));
    }

    let chk_gamemode = gtk4::CheckButton::new();
    chk_gamemode.set_active(!exec_line.contains("USE_GAMEMODE=0"));

    let chk_mangohud = gtk4::CheckButton::new();
    chk_mangohud.set_active(!exec_line.contains("USE_MANGOHUD=0"));

    let wayland_enabled = !exec_line.contains("PROTON_ENABLE_WAYLAND=0");
    let chk_wayland = gtk4::CheckButton::new();
    chk_wayland.set_active(wayland_enabled);

    let chk_steam = gtk4::CheckButton::new();
    chk_steam.set_active(read_desktop_prop(desktop_path, "X-UMU-Steam-Integration") == "1");

    let overlay_enabled = read_desktop_prop(desktop_path, "X-UMU-Steam-Overlay") == "1";
    let chk_overlay = gtk4::CheckButton::new();
    chk_overlay.set_active(overlay_enabled);

    let chk_vpn = gtk4::CheckButton::new();
    chk_vpn.set_active(read_desktop_prop(desktop_path, "X-UMU-VPN") == "1");

    let lock_sig = Rc::new(RefCell::new(false));
    let ls1 = lock_sig.clone();
    let w_chk = chk_wayland.clone();
    chk_overlay.connect_toggled(move |btn| {
        if *ls1.borrow() {
            return;
        }
        *ls1.borrow_mut() = true;
        if btn.is_active() {
            w_chk.set_active(false);
            w_chk.set_sensitive(false);
        } else {
            w_chk.set_sensitive(true);
        }
        *ls1.borrow_mut() = false;
    });

    let ls2 = lock_sig.clone();
    let o_chk = chk_overlay.clone();
    chk_wayland.connect_toggled(move |btn| {
        if *ls2.borrow() {
            return;
        }
        *ls2.borrow_mut() = true;
        if btn.is_active() {
            o_chk.set_active(false);
            o_chk.set_sensitive(false);
        } else {
            o_chk.set_sensitive(true);
        }
        *ls2.borrow_mut() = false;
    });

    if overlay_enabled {
        chk_wayland.set_active(false);
        chk_wayland.set_sensitive(false);
    } else if wayland_enabled {
        chk_overlay.set_active(false);
        chk_overlay.set_sensitive(false);
    }

    let ent_prefix = gtk4::Entry::new();
    ent_prefix.set_text(if prefix_name.is_empty() {
        "default"
    } else {
        &prefix_name
    });

    let cmb_gpu = gtk4::ComboBoxText::new();
    let gpu_opts = ["Автоматически", "AMD", "Nvidia", "Intel"];
    for opt in gpu_opts {
        cmb_gpu.append(Some(opt), opt);
    }
    if !cmb_gpu.set_active_id(Some(&gpu_select)) {
        cmb_gpu.set_active(Some(0));
    }

    let ent_name = gtk4::Entry::new();
    ent_name.set_text(&current_name);

    let img_preview = gtk4::Image::new();
    img_preview.set_pixel_size(48);
    img_preview.set_valign(gtk4::Align::Center);
    let btn_zoom = gtk4::Button::with_label("Увеличить");

    let ip_cb = img_preview.clone();
    let fc_icon = FileChooserButton::new("Выберите иконку", &dlg, move |path| {
        set_image_from_path_or_theme(&ip_cb, &path, 48);
    });

    let init_icon = if current_icon.is_empty() {
        "wine"
    } else {
        &current_icon
    };
    fc_icon.set_filename(init_icon);
    set_image_from_path_or_theme(&img_preview, init_icon, 48);

    let btn_icon_search = gtk4::Button::with_label("Поиск в Steam");
    let btn_icon_reset = gtk4::Button::with_label("Сбросить");

    let icon_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    icon_hbox.append(fc_icon.widget());
    icon_hbox.append(&btn_icon_search);
    icon_hbox.append(&btn_icon_reset);

    let ent_args = gtk4::Entry::new();
    ent_args.set_text(&raw_args);
    ent_args.set_placeholder_text(Some("VAR=1 %command% --dx11"));

    let ent_gameid = gtk4::Entry::new();
    ent_gameid.set_text(&gameid);
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

    add_row(&grid, 0, "Файл (.exe)", exe_hbox.upcast_ref());
    add_row(&grid, 1, "Версия Proton", cmb_proton.upcast_ref());
    add_row(&grid, 2, "GameMode", chk_gamemode.upcast_ref());
    add_row(&grid, 3, "MangoHud", chk_mangohud.upcast_ref());
    add_row(&grid, 4, "Wayland", chk_wayland.upcast_ref());
    add_row(&grid, 5, "Интегр. Steam", chk_steam.upcast_ref());
    add_row(&grid, 6, "Оверлей Steam", chk_overlay.upcast_ref());
    add_row(&grid, 7, "Через VPN", chk_vpn.upcast_ref());
    add_row(&grid, 8, "Префикс (в ~/.umu/)", ent_prefix.upcast_ref());
    add_row(&grid, 9, "Видеокарта", cmb_gpu.upcast_ref());
    add_row(&grid, 10, "Название", ent_name.upcast_ref());
    add_row(&grid, 11, "Иконка (файл)", icon_hbox.upcast_ref());
    add_row(&grid, 12, "Аргументы запуска", ent_args.upcast_ref());
    add_row(&grid, 13, "Game ID / App ID", gameid_hbox.upcast_ref());

    let preview_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    preview_hbox.append(&img_preview);
    preview_hbox.append(&btn_zoom);
    add_row(&grid, 14, "Предпросмотр иконки", preview_hbox.upcast_ref());

    let path_hash = Path::new(desktop_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .replace("umu-", "")
        .replace(".desktop", "");
    let def_spec = get_xdg_data_home()
        .join("icons/umu")
        .join(format!("umu-{}.png", path_hash));
    let def_spec_str = if def_spec.exists() {
        def_spec.to_string_lossy().to_string()
    } else {
        "wine".into()
    };

    let def_s_clone = def_spec_str.clone();
    let fc_r = fc_icon.clone();
    let ip_r = img_preview.clone();
    btn_icon_reset.connect_clicked(move |_| {
        fc_r.set_filename(&def_s_clone);
        set_image_from_path_or_theme(&ip_r, &def_s_clone, 48);
    });

    let w_zm = dlg.clone();
    let fc_z = fc_icon.clone();
    btn_zoom.connect_clicked(move |_| {
        open_zoom_preview(&w_zm, &fc_z.get_filename());
    });

    let w_s_game = dlg.clone();
    let gid_s = ent_gameid.clone();
    let name_s = ent_name.clone();
    let fc_s1 = fc_icon.clone();
    let ip_s1 = img_preview.clone();
    btn_steam_search.connect_clicked(move |_| {
        let g = gid_s.clone();
        let n = name_s.clone();
        let fc = fc_s1.clone();
        let ip = ip_s1.clone();
        open_steam_search_dialog(&w_s_game, move |game_name, appid, icon_path| {
            g.set_text(&appid);
            n.set_text(&game_name);
            if let Some(ref ico) = icon_path {
                fc.set_filename(ico);
                set_image_from_path_or_theme(&ip, ico, 48);
            }
        });
    });

    let w_s_icon = dlg.clone();
    let name_s2 = ent_name.clone();
    let fc_s2 = fc_icon.clone();
    let ip_s2 = img_preview.clone();
    btn_icon_search.connect_clicked(move |_| {
        let n = name_s2.clone();
        let fc = fc_s2.clone();
        let ip = ip_s2.clone();
        open_steam_search_dialog(&w_s_icon, move |game_name, _appid, icon_path| {
            n.set_text(&game_name);
            if let Some(ref ico) = icon_path {
                fc.set_filename(ico);
                set_image_from_path_or_theme(&ip, ico, 48);
            }
        });
    });

    let bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    bbox.set_halign(gtk4::Align::End);
    let btn_save = gtk4::Button::with_label("Сохранить изменения");
    let btn_cancel = gtk4::Button::with_label("Отмена");
    btn_save.set_focusable(false);
    btn_cancel.set_focusable(false);
    bbox.append(&btn_save);
    bbox.append(&btn_cancel);
    vbox.append(&bbox);

    let w_c = dlg.clone();
    btn_cancel.connect_clicked(move |_| w_c.close());

    let d_path = desktop_path.to_string();
    let w_save = dlg.clone();
    let on_s = Rc::new(on_saved);
    let fc_save = fc_icon.clone();

    btn_save.connect_clicked(move |_| {
        let mut new_name = ent_name.text().to_string();
        let new_exe = exe_entry.text().to_string();
        let new_icon = persist_icon(&fc_save.get_filename());
        let new_args = ent_args.text().to_string();
        let new_prefix = ent_prefix.text().to_string();
        let new_gpu = cmb_gpu.active_text().unwrap_or_default().to_string();
        let new_proton = cmb_proton.active_text().unwrap_or_default().to_string();
        let new_gameid = ent_gameid.text().to_string();

        let env_gamemode = if chk_gamemode.is_active() { "1" } else { "0" };
        let env_mangohud = if chk_mangohud.is_active() { "1" } else { "0" };
        let env_wayland = if chk_wayland.is_active() { "1" } else { "0" };
        let env_steam = if chk_steam.is_active() { "1" } else { "0" };
        let env_overlay = if chk_overlay.is_active() { "1" } else { "0" };
        let env_vpn = if chk_vpn.is_active() { "1" } else { "0" };

        if Path::new(&new_exe).exists() && new_name.ends_with(" (Inactive)") {
            new_name = new_name.trim_end_matches(" (Inactive)").trim().to_string();
        }

        let env_base = format!(
            "env GAMEID={} USE_GAMEMODE={} USE_MANGOHUD={} PROTON_ENABLE_WAYLAND={} UMU_PREFIX_NAME={} UMU_PROTON_TYPE=\"{}\" USE_STEAM_INTEGRATION={} USE_STEAM_OVERLAY={} USE_VPN={} UMU_GPU_SELECT=\"{}\"",
            new_gameid, env_gamemode, env_mangohud, env_wayland, new_prefix, new_proton, env_steam, env_overlay, env_vpn, new_gpu
        );

        let exec_cmd = if new_args.contains("%command%") {
            let parts: Vec<&str> = new_args.splitn(2, "%command%").collect();
            format!(
                "{} {} umu-run-wrapper \"{}\" {}",
                env_base,
                parts[0].trim(),
                new_exe,
                parts.get(1).unwrap_or(&"").trim()
            )
        } else {
            format!("{} umu-run-wrapper \"{}\" {}", env_base, new_exe, new_args.trim())
        };

        let exe_dir = Path::new(&new_exe)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_string_lossy()
            .to_string();

        write_desktop_props(
            &d_path,
            &[
                ("Name", &new_name),
                ("Icon", &new_icon),
                ("Exec", &exec_cmd),
                ("Path", &exe_dir),
                ("X-UMU-Actual-Exe", &new_exe),
                ("X-UMU-Raw-Args", &new_args),
                ("X-UMU-Prefix-Name", &new_prefix),
                ("X-UMU-GPU-Select", &new_gpu),
                ("X-UMU-Steam-Integration", env_steam),
                ("X-UMU-Steam-Overlay", env_overlay),
                ("X-UMU-Proton-Type", &new_proton),
                ("X-UMU-VPN", env_vpn),
                ("X-UMU-Game-ID", &new_gameid),
            ],
        );

        on_s();
        w_save.close();
    });

    dlg.present();
}
