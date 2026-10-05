use gtk4::prelude::*;
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use system_ui_helpers::{
    FileChooserButton, PathsListWidget, apply_clean_theme, extract_default_icon_for_exe,
    get_cached_icons_dir, get_xdg_data_home, load_proton_versions, open_steam_search_dialog,
    open_zoom_preview, persist_icon, persist_icon_as, read_desktop_prop,
    set_image_from_path_or_theme, write_desktop_props,
};

fn populate_shortcuts(listbox: &gtk4::ListBox) {
    while let Some(child) = listbox.first_child() {
        listbox.remove(&child);
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
                row.set_widget_name(&format!("{dname}|{path}"));

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
                listbox.append(&row);
            }
        }
    }
}

fn setup_shortcuts_window() -> (
    gtk4::Window,
    gtk4::SearchEntry,
    gtk4::ListBox,
    gtk4::Button,
    gtk4::Button,
    gtk4::Button,
) {
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

    let bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    bbox.set_halign(gtk4::Align::End);
    let btn_create = gtk4::Button::with_label("Создать ярлык");
    btn_create.add_css_class("suggested-action");
    let btn_delete = gtk4::Button::with_label("Удалить ярлык");
    let btn_close = gtk4::Button::with_label("Закрыть");

    btn_create.set_focusable(false);
    btn_create.set_can_focus(false);
    btn_delete.set_focusable(false);
    btn_delete.set_can_focus(false);
    btn_close.set_focusable(false);
    btn_close.set_can_focus(false);

    bbox.append(&btn_create);
    bbox.append(&btn_delete);
    bbox.append(&btn_close);
    vbox.append(&bbox);

    (
        win,
        search_entry,
        listbox,
        btn_create,
        btn_delete,
        btn_close,
    )
}

fn connect_main_actions(
    win: &gtk4::Window,
    listbox: &gtk4::ListBox,
    btn_create: &gtk4::Button,
    btn_delete: &gtk4::Button,
    get_selected: &Rc<dyn Fn() -> Option<(String, String)>>,
) {
    let lb_create = listbox.clone();
    btn_create.connect_clicked(move |_| {
        let lb = lb_create.clone();
        let mut child = std::process::Command::new("run-exe");
        if let Ok(mut c) = child.spawn() {
            glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                match c.try_wait() {
                    Ok(Some(_)) => {
                        populate_shortcuts(&lb);
                        glib::ControlFlow::Break
                    }
                    Ok(None) => glib::ControlFlow::Continue,
                    Err(_) => glib::ControlFlow::Break,
                }
            });
        }
    });

    let w_del = win.clone();
    let gs_del = Rc::clone(get_selected);
    let lb_del = listbox.clone();
    btn_delete.connect_clicked(move |_| {
        if let Some((name, desk_path)) = gs_del() {
            let dlg = gtk4::MessageDialog::builder()
                .transient_for(&w_del)
                .modal(true)
                .message_type(gtk4::MessageType::Question)
                .buttons(gtk4::ButtonsType::YesNo)
                .text(format!(
                    "Вы уверены, что хотите полностью удалить ярлык '{name}'?"
                ))
                .build();

            let lb_c = lb_del.clone();
            dlg.connect_response(move |d, response| {
                if response == gtk4::ResponseType::Yes {
                    fs::remove_file(&desk_path).ok();
                    populate_shortcuts(&lb_c);
                }
                d.destroy();
            });
            dlg.present();
        }
    });
}

fn main() {
    gtk4::init().expect("GTK4 init failed");
    apply_clean_theme();
    let main_loop = glib::MainLoop::new(None, false);

    let (win, search_entry, listbox, btn_create, btn_delete, btn_close) = setup_shortcuts_window();

    populate_shortcuts(&listbox);

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

    let w_close = win.clone();
    btn_close.connect_clicked(move |_| w_close.close());

    let get_selected: Rc<dyn Fn() -> Option<(String, String)>> = {
        let lb = listbox.clone();
        Rc::new(move || -> Option<(String, String)> {
            if let Some(row) = lb.selected_row() {
                let data = row.widget_name().to_string();
                if let Some((name, path)) = data.split_once('|') {
                    return Some((name.to_string(), path.to_string()));
                }
            }
            None
        })
    };

    let w_edit = win.clone();
    let gs_edit = Rc::clone(&get_selected);
    let lb_edit = listbox.clone();
    listbox.connect_row_activated(move |_, _| {
        if let Some((_, desk_path)) = gs_edit() {
            let lb = lb_edit.clone();
            open_edit_dialog(&w_edit, &desk_path, move || populate_shortcuts(&lb));
        }
    });

    connect_main_actions(&win, &listbox, &btn_create, &btn_delete, &get_selected);

    let ml_win = main_loop.clone();
    win.connect_close_request(move |_| {
        ml_win.quit();
        glib::Propagation::Proceed
    });

    search_entry.grab_focus();
    win.present();
    main_loop.run();
}

struct EditorToggles {
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

struct EditorWidgets {
    exe_entry: gtk4::Entry,
    cmb_proton: gtk4::ComboBoxText,
    toggles: EditorToggles,
    ent_prefix: gtk4::Entry,
    cmb_gpu: gtk4::ComboBoxText,
    ent_name: gtk4::Entry,
    ent_args: gtk4::Entry,
    ent_gameid: gtk4::Entry,
    fc_icon: FileChooserButton,
    paths_widget: PathsListWidget,
}

fn resolve_shortcut_icon(fc_icon: &FileChooserButton, path_hash: &str) -> String {
    let sel_icon = fc_icon.get_filename();
    let cache_dir_str = get_cached_icons_dir().to_string_lossy().to_string();
    if sel_icon.starts_with(&cache_dir_str) && sel_icon.contains("umu-") {
        persist_icon_as(&sel_icon, Some(&format!("umu-{path_hash}.png")))
    } else {
        persist_icon(&sel_icon)
    }
}

struct ShortcutValues<'a> {
    name: &'a str,
    icon: &'a str,
    exe: &'a str,
    args: &'a str,
    exe_dir: &'a str,
    prefix: &'a str,
    gpu: &'a str,
    proton: &'a str,
    gameid: &'a str,
    extra_paths: &'a str,
}

fn get_shortcut_props<'a>(
    widgets: &'a EditorWidgets,
    vals: &'a ShortcutValues<'a>,
) -> [(&'static str, &'a str); 21] {
    let bool_str = |b: bool| if b { "1" } else { "0" };
    [
        ("Name", vals.name),
        ("Icon", vals.icon),
        ("Exec", "umu-run-wrapper %k"),
        ("Path", vals.exe_dir),
        ("X-UMU-Actual-Exe", vals.exe),
        ("X-UMU-Raw-Args", vals.args),
        ("X-UMU-Prefix-Name", vals.prefix),
        ("X-UMU-GPU-Select", vals.gpu),
        (
            "X-UMU-Gamemode",
            bool_str(widgets.toggles.gamemode.is_active()),
        ),
        (
            "X-UMU-Mangohud",
            bool_str(widgets.toggles.mangohud.is_active()),
        ),
        (
            "X-UMU-Wayland",
            bool_str(widgets.toggles.wayland.is_active()),
        ),
        (
            "X-UMU-Steam-Integration",
            bool_str(widgets.toggles.steam.is_active()),
        ),
        (
            "X-UMU-Steam-Overlay",
            bool_str(widgets.toggles.overlay.is_active()),
        ),
        ("X-UMU-Proton-Type", vals.proton),
        ("X-UMU-VPN", bool_str(widgets.toggles.vpn.is_active())),
        ("X-UMU-Game-ID", vals.gameid),
        (
            "X-UMU-Sandbox",
            bool_str(widgets.toggles.sandbox.is_active()),
        ),
        (
            "X-UMU-Gamepad",
            bool_str(widgets.toggles.gamepad.is_active()),
        ),
        (
            "X-UMU-Network",
            bool_str(widgets.toggles.network.is_active()),
        ),
        (
            "X-UMU-Steam-Ports",
            bool_str(widgets.toggles.steam_ports.is_active()),
        ),
        ("X-UMU-Extra-Paths", vals.extra_paths),
    ]
}

fn save_shortcut_edits(
    desktop_path: &str,
    widgets: &EditorWidgets,
    path_hash: &str,
    on_saved: &dyn Fn(),
) {
    let mut new_name = widgets.ent_name.text().to_string();
    let new_exe = widgets.exe_entry.text().to_string();
    let new_icon = resolve_shortcut_icon(&widgets.fc_icon, path_hash);
    let new_args = widgets.ent_args.text().to_string();

    let extra_paths = widgets.paths_widget.to_string_args();
    let new_prefix = widgets.ent_prefix.text().to_string();
    let new_gpu = widgets
        .cmb_gpu
        .active_text()
        .unwrap_or_default()
        .to_string();
    let new_proton = widgets
        .cmb_proton
        .active_text()
        .unwrap_or_default()
        .to_string();
    let new_gameid = widgets.ent_gameid.text().to_string();

    if Path::new(&new_exe).exists() && new_name.ends_with(" (Inactive)") {
        new_name = new_name.trim_end_matches(" (Inactive)").trim().to_string();
    }

    let exe_dir = Path::new(&new_exe)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_string_lossy()
        .to_string();

    let vals = ShortcutValues {
        name: &new_name,
        icon: &new_icon,
        exe: &new_exe,
        args: &new_args,
        exe_dir: &exe_dir,
        prefix: &new_prefix,
        gpu: &new_gpu,
        proton: &new_proton,
        gameid: &new_gameid,
        extra_paths: &extra_paths,
    };

    let props = get_shortcut_props(widgets, &vals);

    write_desktop_props(desktop_path, &props);

    on_saved();
}

fn connect_wayland_overlay_locks(
    chk_wayland: &gtk4::CheckButton,
    chk_overlay: &gtk4::CheckButton,
    wayland_enabled: bool,
    overlay_enabled: bool,
) {
    let lock_sig = Rc::new(RefCell::new(false));
    let ls1 = Rc::clone(&lock_sig);
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

    let ls2 = lock_sig;
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
}

fn build_editor_toggles(
    exec_line: &str,
    desktop_path: &str,
    sandbox_enabled: bool,
    gamepad_enabled: bool,
) -> EditorToggles {
    let prop_or_exec = |key: &str, exec_match: &str, default_true: bool| {
        let p = read_desktop_prop(desktop_path, key);
        if !p.is_empty() {
            p != "0"
        } else if default_true {
            !exec_line.contains(&format!("{exec_match}=0"))
        } else {
            exec_line.contains(&format!("{exec_match}=1"))
        }
    };

    let chk_gamemode = gtk4::CheckButton::new();
    chk_gamemode.set_active(prop_or_exec("X-UMU-Gamemode", "USE_GAMEMODE", true));

    let chk_mangohud = gtk4::CheckButton::new();
    chk_mangohud.set_active(prop_or_exec("X-UMU-Mangohud", "USE_MANGOHUD", true));

    let wayland_enabled = prop_or_exec("X-UMU-Wayland", "PROTON_ENABLE_WAYLAND", true);
    let chk_wayland = gtk4::CheckButton::new();
    chk_wayland.set_active(wayland_enabled);

    let chk_steam = gtk4::CheckButton::new();
    chk_steam.set_active(prop_or_exec(
        "X-UMU-Steam-Integration",
        "USE_STEAM_INTEGRATION",
        false,
    ));

    let overlay_enabled = prop_or_exec("X-UMU-Steam-Overlay", "USE_STEAM_OVERLAY", false);
    let chk_overlay = gtk4::CheckButton::new();
    chk_overlay.set_active(overlay_enabled);

    let chk_vpn = gtk4::CheckButton::new();
    chk_vpn.set_active(prop_or_exec("X-UMU-VPN", "USE_VPN", false));

    let chk_sandbox = gtk4::CheckButton::new();
    chk_sandbox.set_active(prop_or_exec(
        "X-UMU-Sandbox",
        "USE_SANDBOX",
        sandbox_enabled,
    ));

    let chk_gamepad = gtk4::CheckButton::new();
    chk_gamepad.set_active(prop_or_exec(
        "X-UMU-Gamepad",
        "USE_GAMEPAD",
        gamepad_enabled,
    ));

    let network_enabled = prop_or_exec("X-UMU-Network", "USE_NETWORK", true);
    let chk_network = gtk4::CheckButton::new();
    chk_network.set_active(network_enabled);

    let sp_prop = read_desktop_prop(desktop_path, "X-UMU-Steam-Ports");
    let steam_ports_enabled = if sp_prop.is_empty() {
        chk_steam.is_active()
    } else {
        sp_prop == "1"
    };
    let chk_steam_ports = gtk4::CheckButton::new();
    chk_steam_ports.set_active(steam_ports_enabled);

    connect_wayland_overlay_locks(&chk_wayland, &chk_overlay, wayland_enabled, overlay_enabled);

    EditorToggles {
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

fn build_icon_controls(
    dlg: &gtk4::Window,
    current_icon: &str,
    actual_exe: &str,
    exe_entry: &gtk4::Entry,
    ent_prefix: &gtk4::Entry,
    ent_name: &gtk4::Entry,
    ent_gameid: &gtk4::Entry,
) -> (FileChooserButton, gtk4::Image, gtk4::Box) {
    let img_preview = gtk4::Image::new();
    img_preview.set_pixel_size(48);
    img_preview.set_valign(gtk4::Align::Center);

    let ip_cb = img_preview.clone();
    let fc_icon = FileChooserButton::new("Выберите иконку", dlg, move |path| {
        set_image_from_path_or_theme(&ip_cb, &path, 48);
    });

    let init_icon = if current_icon.is_empty() {
        "wine"
    } else {
        current_icon
    };
    fc_icon.set_filename(init_icon);
    set_image_from_path_or_theme(&img_preview, init_icon, 48);

    let btn_icon_search = gtk4::Button::with_label("Поиск в Steam");
    let btn_icon_reset = gtk4::Button::with_label("Сбросить");

    let icon_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    icon_hbox.append(fc_icon.widget());
    icon_hbox.append(&btn_icon_search);
    icon_hbox.append(&btn_icon_reset);

    let fc_reset = fc_icon.clone();
    let img_reset = img_preview.clone();
    let exe_reset = exe_entry.clone();
    let pfx_reset = ent_prefix.clone();
    let def_exe = actual_exe.to_string();
    btn_icon_reset.connect_clicked(move |_| {
        let mut target = exe_reset.text().to_string();
        if target.is_empty() || !Path::new(&target).exists() {
            target.clone_from(&def_exe);
        }
        let chosen = if target.is_empty() {
            "wine".to_string()
        } else {
            extract_default_icon_for_exe(&target, &pfx_reset.text())
                .unwrap_or_else(|| "wine".to_string())
        };
        fc_reset.set_filename(&chosen);
        set_image_from_path_or_theme(&img_reset, &chosen, 48);
    });

    let w_steam = dlg.clone();
    let fc_steam = fc_icon.clone();
    let img_steam = img_preview.clone();
    let name_steam = ent_name.clone();
    let gid_steam = ent_gameid.clone();
    btn_icon_search.connect_clicked(move |_| {
        let game_id_ent = gid_steam.clone();
        let name_ent = name_steam.clone();
        let fc = fc_steam.clone();
        let img = img_steam.clone();
        open_steam_search_dialog(&w_steam, move |game_name, appid, icon_path| {
            game_id_ent.set_text(&appid);
            name_ent.set_text(&game_name);
            if let Some(ref ico) = icon_path {
                fc.set_filename(ico);
                set_image_from_path_or_theme(&img, ico, 48);
            }
        });
    });

    (fc_icon, img_preview, icon_hbox)
}

fn build_exe_browse_hbox(dlg: &gtk4::Window, actual_exe: &str) -> (gtk4::Box, gtk4::Entry) {
    let exe_entry = gtk4::Entry::new();
    exe_entry.set_text(actual_exe);
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
            if res == gtk4::ResponseType::Accept
                && let Some(f) = dialog.file()
                && let Some(p) = f.path()
            {
                ec.set_text(&p.to_string_lossy());
            }
            dialog.destroy();
        });
        d.show();
    });

    (exe_hbox, exe_entry)
}

struct EditorBoxes {
    exe: gtk4::Box,
    icon: gtk4::Box,
    gameid: gtk4::Box,
}

fn attach_editor_rows(grid: &gtk4::Grid, widgets: &EditorWidgets, boxes: &EditorBoxes) {
    let add_row = |g: &gtk4::Grid, row: i32, title: &str, widget: &gtk4::Widget| {
        let lbl = gtk4::Label::builder()
            .label(title)
            .xalign(0.0)
            .yalign(0.5)
            .build();
        g.attach(&lbl, 0, row, 1, 1);
        g.attach(widget, 1, row, 1, 1);
    };

    add_row(grid, 0, "Файл (.exe)", boxes.exe.upcast_ref());
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
    add_row(grid, 10, "Название", widgets.ent_name.upcast_ref());
    add_row(grid, 11, "Иконка (файл)", boxes.icon.upcast_ref());
    add_row(grid, 12, "Аргументы запуска", widgets.ent_args.upcast_ref());
    add_row(grid, 13, "Game ID / App ID", boxes.gameid.upcast_ref());
    add_row(grid, 14, "Песочница", widgets.toggles.sandbox.upcast_ref());
    add_row(
        grid,
        15,
        "Сеть песочницы",
        widgets.toggles.network.upcast_ref(),
    );
    add_row(
        grid,
        16,
        "Порты Steam",
        widgets.toggles.steam_ports.upcast_ref(),
    );
    add_row(grid, 17, "Геймпад", widgets.toggles.gamepad.upcast_ref());
}

fn build_editor_combos(
    proton_type: &str,
    gpu_select: &str,
) -> (gtk4::ComboBoxText, gtk4::ComboBoxText) {
    let proton_versions = load_proton_versions();
    let cmb_proton = gtk4::ComboBoxText::new();
    for v in &proton_versions {
        cmb_proton.append(Some(&v.name), &v.name);
    }
    if !cmb_proton.set_active_id(Some(proton_type)) {
        cmb_proton.set_active(Some(0));
    }

    let cmb_gpu = gtk4::ComboBoxText::new();
    let gpu_opts = ["Автоматически", "AMD", "Nvidia", "Intel"];
    for opt in gpu_opts {
        cmb_gpu.append(Some(opt), opt);
    }
    if !cmb_gpu.set_active_id(Some(gpu_select)) {
        cmb_gpu.set_active(Some(0));
    }

    (cmb_proton, cmb_gpu)
}

fn connect_editor_steam_search(
    dlg: &gtk4::Window,
    btn_steam_search: &gtk4::Button,
    fc_icon: &FileChooserButton,
    img_preview: &gtk4::Image,
    ent_name: &gtk4::Entry,
    ent_gameid: &gtk4::Entry,
) {
    let w_s2 = dlg.clone();
    let fc_s2 = fc_icon.clone();
    let img_s2 = img_preview.clone();
    let name_s2 = ent_name.clone();
    let gid_s2 = ent_gameid.clone();
    btn_steam_search.connect_clicked(move |_| {
        let game_id_ent = gid_s2.clone();
        let name_ent = name_s2.clone();
        let fc = fc_s2.clone();
        let img = img_s2.clone();
        open_steam_search_dialog(&w_s2, move |game_name, appid, icon_path| {
            game_id_ent.set_text(&appid);
            name_ent.set_text(&game_name);
            if let Some(ref ico) = icon_path {
                fc.set_filename(ico);
                set_image_from_path_or_theme(&img, ico, 48);
            }
        });
    });
}

struct DesktopShortcutProps {
    current_name: String,
    current_icon: String,
    raw_args: String,
    actual_exe: String,
    prefix_name: String,
    gpu_select: String,
    exec_line: String,
    proton_type: String,
    gameid: String,
    sandbox_enabled: bool,
    gamepad_enabled: bool,
    extra_paths: String,
}

fn load_shortcut_props(desktop_path: &str) -> DesktopShortcutProps {
    let current_name = read_desktop_prop(desktop_path, "Name");
    let current_icon = read_desktop_prop(desktop_path, "Icon");
    let raw_args = read_desktop_prop(desktop_path, "X-UMU-Raw-Args");
    let actual_exe = read_desktop_prop(desktop_path, "X-UMU-Actual-Exe");
    let prefix_name = read_desktop_prop(desktop_path, "X-UMU-Prefix-Name");
    let gpu_select = read_desktop_prop(desktop_path, "X-UMU-GPU-Select");
    let exec_line = read_desktop_prop(desktop_path, "Exec");
    let proton_type = read_desktop_prop(desktop_path, "X-UMU-Proton-Type");
    let gameid = read_desktop_prop(desktop_path, "X-UMU-Game-ID");
    let sandbox_prop = read_desktop_prop(desktop_path, "X-UMU-Sandbox");
    let gamepad_prop = read_desktop_prop(desktop_path, "X-UMU-Gamepad");
    let extra_paths = read_desktop_prop(desktop_path, "X-UMU-Extra-Paths");

    let sandbox_enabled = if sandbox_prop.is_empty() {
        !exec_line.contains("USE_SANDBOX=0")
    } else {
        sandbox_prop != "0"
    };

    let gamepad_enabled = if gamepad_prop.is_empty() {
        !exec_line.contains("USE_GAMEPAD=0")
    } else {
        gamepad_prop != "0"
    };

    DesktopShortcutProps {
        current_name,
        current_icon,
        raw_args,
        actual_exe,
        prefix_name,
        gpu_select,
        exec_line,
        proton_type,
        gameid,
        sandbox_enabled,
        gamepad_enabled,
        extra_paths,
    }
}

fn build_editor_grid(
    dlg: &gtk4::Window,
    desktop_path: &str,
) -> (gtk4::Grid, EditorWidgets, gtk4::Image, String) {
    let props = load_shortcut_props(desktop_path);

    let grid = gtk4::Grid::new();
    grid.set_column_spacing(15);
    grid.set_row_spacing(10);

    let (exe_hbox, exe_entry) = build_exe_browse_hbox(dlg, &props.actual_exe);

    let (cmb_proton, cmb_gpu) = build_editor_combos(&props.proton_type, &props.gpu_select);
    let toggles = build_editor_toggles(
        &props.exec_line,
        desktop_path,
        props.sandbox_enabled,
        props.gamepad_enabled,
    );

    let ent_prefix = gtk4::Entry::new();
    ent_prefix.set_text(if props.prefix_name.is_empty() {
        "default"
    } else {
        &props.prefix_name
    });

    let ent_name = gtk4::Entry::new();
    ent_name.set_text(&props.current_name);

    let ent_gameid = gtk4::Entry::new();
    ent_gameid.set_text(&props.gameid);
    ent_gameid.set_placeholder_text(Some("Например: 292030 (AppID)"));
    ent_gameid.set_hexpand(true);

    let (fc_icon, img_preview, icon_hbox) = build_icon_controls(
        dlg,
        &props.current_icon,
        &props.actual_exe,
        &exe_entry,
        &ent_prefix,
        &ent_name,
        &ent_gameid,
    );

    let ent_args = gtk4::Entry::new();
    ent_args.set_text(&props.raw_args);
    ent_args.set_placeholder_text(Some("VAR=1 %command% --dx11"));

    let btn_steam_search = gtk4::Button::with_label("Поиск в Steam");
    let gameid_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    gameid_hbox.append(&ent_gameid);
    gameid_hbox.append(&btn_steam_search);

    connect_editor_steam_search(
        dlg,
        &btn_steam_search,
        &fc_icon,
        &img_preview,
        &ent_name,
        &ent_gameid,
    );

    let paths_widget = PathsListWidget::new(dlg);
    if !props.extra_paths.trim().is_empty() {
        paths_widget.load_from_string_args(&props.extra_paths);
    }

    let widgets = EditorWidgets {
        exe_entry,
        cmb_proton,
        toggles,
        ent_prefix,
        cmb_gpu,
        ent_name,
        ent_args,
        ent_gameid,
        fc_icon,
        paths_widget,
    };

    let boxes = EditorBoxes {
        exe: exe_hbox,
        icon: icon_hbox,
        gameid: gameid_hbox,
    };

    attach_editor_rows(&grid, &widgets, &boxes);

    (grid, widgets, img_preview, props.current_name)
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

    let (grid, widgets, img_preview, current_name) = build_editor_grid(&dlg, desktop_path);

    let title_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    let lbl_p = gtk4::Label::builder()
        .label("<b>Редактирование:</b>")
        .use_markup(true)
        .build();
    let lbl_f = gtk4::Label::builder()
        .label(format!("<b>{current_name}</b>"))
        .use_markup(true)
        .build();
    title_hbox.append(&lbl_p);
    title_hbox.append(&lbl_f);
    vbox.append(&title_hbox);
    vbox.append(&grid);

    let paths_expander = gtk4::Expander::builder()
        .label("Дополнительные каталоги (Монтирование)")
        .child(widgets.paths_widget.widget())
        .build();
    vbox.append(&paths_expander);

    let btn_zoom = gtk4::Button::with_label("Увеличить");
    let preview_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    preview_hbox.append(&img_preview);
    preview_hbox.append(&btn_zoom);
    grid.attach(
        &gtk4::Label::builder()
            .label("Предпросмотр иконки")
            .xalign(0.0)
            .build(),
        0,
        16,
        1,
        1,
    );
    grid.attach(&preview_hbox, 1, 16, 1, 1);

    let path_hash = Path::new(desktop_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .replace("umu-", "")
        .replace(".desktop", "");

    let w_zm = dlg.clone();
    let fc_z = widgets.fc_icon.clone();
    btn_zoom.connect_clicked(move |_| {
        open_zoom_preview(&w_zm, &fc_z.get_filename());
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

    btn_save.connect_clicked(move |_| {
        save_shortcut_edits(&d_path, &widgets, &path_hash, &*on_s);
        w_save.close();
    });

    dlg.present();
}
