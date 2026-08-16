use gtk::prelude::*;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// SATTY'S 1ms FLOATING HACK ADAPTED FOR GTK3
fn apply_floating_hack(window: &impl IsA<gtk::Window>) {
    window.set_resizable(false);
    let w = window.upcast_ref::<gtk::Window>().downgrade();
    glib::timeout_add_local_once(std::time::Duration::from_millis(1), move || {
        if let Some(win) = w.upgrade() {
            win.set_resizable(true);
        }
    });
}

fn main() {
    gtk::init().expect("Failed to initialize GTK.");

    let args: Vec<String> = env::args().collect();
    let app_name = std::path::Path::new(&args[0])
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default();

    if app_name == "fix-umu-path" {
        if let Some(path) = args.get(1) {
            fix_path(path);
        }
    } else if app_name == "manage-umu-prefixes" {
        manage_prefixes();
    } else if app_name == "manage-umu-shortcuts" {
        manage_shortcuts();
    } else {
        // Default to run-exe
        let path = args.get(1).cloned().unwrap_or_default();
        run_exe(&path);
    }

    gtk::main();
}

// =========================================================================
// DESKTOP FILE HELPERS
// =========================================================================
fn read_desktop_prop(path: &str, key: &str) -> String {
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            if line.starts_with(&format!("{}=", key)) {
                return line.split_once('=').unwrap().1.to_string();
            }
        }
    }
    String::new()
}

fn write_desktop_props(path: &str, props: &[(&str, &str)]) {
    if let Ok(content) = std::fs::read_to_string(path) {
        let mut new_lines = Vec::new();
        let mut keys_written = std::collections::HashSet::new();

        for line in content.lines() {
            let mut matched = false;
            for (k, v) in props {
                if line.starts_with(&format!("{}=", k)) {
                    new_lines.push(format!("{}={}", k, v));
                    keys_written.insert(*k);
                    matched = true;
                    break;
                }
            }
            if !matched {
                new_lines.push(line.to_string());
            }
        }

        for (k, v) in props {
            if !keys_written.contains(k) {
                new_lines.push(format!("{}={}", k, v));
            }
        }

        let _ = std::fs::write(path, new_lines.join("\n"));
    }
}

// =========================================================================
// 1. FIX PATH
// =========================================================================
fn fix_path(desktop_path: &str) {
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_title("Исправление пути к файлу - UMU");
    win.set_default_size(520, -1);
    win.set_position(gtk::WindowPosition::Center);
    win.set_border_width(15);
    apply_floating_hack(&win);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
    win.add(&vbox);

    let current_name = read_desktop_prop(desktop_path, "Name").replace(" (Inactive)", "");
    let actual_exe = read_desktop_prop(desktop_path, "X-UMU-Actual-Exe");

    let lbl = gtk::Label::new(None);
    lbl.set_markup(&format!(
        "<b>Файл запуска для '{}' не найден!</b>\n<span foreground='gray'>Текущий путь: {}</span>\n\nУкажите новое местоположение:",
        current_name, actual_exe
    ));
    lbl.set_xalign(0.0);
    vbox.pack_start(&lbl, false, false, 0);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    let entry = gtk::Entry::new();
    entry.set_text(&actual_exe);
    hbox.pack_start(&entry, true, true, 0);

    let btn_browse = gtk::Button::with_label("Обзор...");
    let w_clone = win.clone();
    let e_clone = entry.clone();
    btn_browse.connect_clicked(move |_| {
        let dlg = gtk::FileChooserDialog::new(Some("Выберите .exe"), Some(&w_clone), gtk::FileChooserAction::Open);
        dlg.add_buttons(&[("Отмена", gtk::ResponseType::Cancel), ("Ок", gtk::ResponseType::Ok)]);
        if dlg.run() == gtk::ResponseType::Ok {
            if let Some(f) = dlg.filename() {
                e_clone.set_text(&f.to_string_lossy());
            }
        }
        dlg.destroy();
    });
    hbox.pack_start(&btn_browse, false, false, 0);
    vbox.pack_start(&hbox, false, false, 0);

    let bbox = gtk::ButtonBox::new(gtk::Orientation::Horizontal);
    bbox.set_layout(gtk::ButtonBoxStyle::End);
    let btn_ok = gtk::Button::with_label("Сохранить и активировать");
    
    let dp = desktop_path.to_string();
    btn_ok.connect_clicked(move |_| {
        let new_exe = entry.text().to_string();
        if !std::path::Path::new(&new_exe).exists() {
            Command::new("notify-send").args(["-u", "critical", "Ошибка", "Файл не существует!"]).spawn().ok();
            return;
        }

        let prefix = read_desktop_prop(&dp, "X-UMU-Prefix-Name");
        let proton = read_desktop_prop(&dp, "X-UMU-Proton-Type");
        let gameid = read_desktop_prop(&dp, "X-UMU-Game-ID");
        let args = read_desktop_prop(&dp, "X-UMU-Raw-Args");

        let env_base = format!("env GAMEID={} USE_GAMEMODE=1 USE_MANGOHUD=1 PROTON_ENABLE_WAYLAND=1 UMU_PREFIX_NAME={} UMU_PROTON_TYPE=\"{}\"", gameid, prefix, proton);
        
        let exec_cmd = if args.contains("%command%") {
            let parts: Vec<&str> = args.splitn(2, "%command%").collect();
            format!("{} {} umu-run-wrapper \"{}\" {}", env_base, parts[0], new_exe, parts.get(1).unwrap_or(&""))
        } else {
            format!("{} umu-run-wrapper \"{}\" {}", env_base, new_exe, args)
        };

        let parent_dir = std::path::Path::new(&new_exe).parent().unwrap().to_string_lossy();
        write_desktop_props(&dp, &[
            ("Name", &current_name),
            ("X-UMU-Actual-Exe", &new_exe),
            ("Exec", &exec_cmd),
            ("Path", &parent_dir),
        ]);

        Command::new("notify-send").args(["Ярлык обновлен", "Путь успешно изменен"]).spawn().ok();
        gtk::main_quit();
    });

    bbox.pack_start(&btn_ok, true, true, 0);
    vbox.pack_start(&bbox, false, false, 0);

    win.connect_delete_event(|_, _| { gtk::main_quit(); Inhibit(false) });
    win.show_all();
}

// =========================================================================
// 2. MANAGE PREFIXES
// =========================================================================
fn manage_prefixes() {
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_title("UMU Prefix Manager");
    win.set_default_size(500, 350);
    win.set_position(gtk::WindowPosition::Center);
    win.set_border_width(10);
    apply_floating_hack(&win);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    win.add(&vbox);

    let lbl = gtk::Label::new(None);
    lbl.set_markup("<b>Выберите префикс для настройки или управления:</b>");
    lbl.set_xalign(0.0);
    vbox.pack_start(&lbl, false, false, 5);

    let store = gtk::ListStore::new(&[glib::Type::STRING, glib::Type::STRING]);
    let treeview = gtk::TreeView::with_model(&store);
    let col = gtk::TreeViewColumn::new();
    col.set_title("Префикс");
    col.set_expand(true);
    let renderer = gtk::CellRendererText::new();
    col.pack_start(&renderer, true);
    col.add_attribute(&renderer, "text", 0);
    treeview.append_column(&col);

    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.add(&treeview);
    vbox.pack_start(&scroll, true, true, 0);

    let umu_dir = format!("{}/.umu", env::var("HOME").unwrap_or_default());
    let populate = {
        let store = store.clone();
        let umu_dir = umu_dir.clone();
        move || {
            store.clear();
            if let Ok(entries) = std::fs::read_dir(&umu_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if entry.path().is_dir() && name != "steamrt3" && name != "umu" {
                        store.insert_with_values(None, &[(0, &name), (1, &entry.path().to_string_lossy().into_owned())]);
                    }
                }
            }
        }
    };
    populate();

    let bbox = gtk::ButtonBox::new(gtk::Orientation::Horizontal);
    bbox.set_layout(gtk::ButtonBoxStyle::End);

    let btn_wt = gtk::Button::with_label("Winetricks");
    let btn_open = gtk::Button::with_label("Открыть папку");
    let btn_del = gtk::Button::with_label("Удалить");

    let get_selected = {
        let treeview = treeview.clone();
        let store = store.clone();
        move || -> Option<(String, String)> {
            if let Some((_, iter)) = treeview.selection().selected() {
                Some((store.value(&iter, 0).get().unwrap(), store.value(&iter, 1).get().unwrap()))
            } else { None }
        }
    };

    let gs1 = get_selected.clone();
    btn_wt.connect_clicked(move |_| {
        if let Some((_, path)) = gs1() {
            Command::new("umu-run-wrapper").args(["winetricks", "--gui"]).env("WINEPREFIX", path).spawn().ok();
        }
    });

    let gs2 = get_selected.clone();
    btn_open.connect_clicked(move |_| {
        if let Some((_, path)) = gs2() {
            Command::new("xdg-open").arg(format!("{}/drive_c", path)).spawn().ok();
        }
    });

    let win_clone = win.clone();
    let gs3 = get_selected.clone();
    btn_del.connect_clicked(move |_| {
        if let Some((name, path)) = gs3() {
            if name == "default" {
                return; // Protection
            }
            let dlg = gtk::MessageDialog::new(Some(&win_clone), gtk::DialogFlags::MODAL, gtk::MessageType::Question, gtk::ButtonsType::YesNo, &format!("Удалить префикс {}?", name));
            if dlg.run() == gtk::ResponseType::Yes {
                std::fs::remove_dir_all(&path).ok();
                populate();
            }
            dlg.destroy();
        }
    });

    bbox.pack_start(&btn_wt, true, true, 0);
    bbox.pack_start(&btn_open, true, true, 0);
    bbox.pack_start(&btn_del, true, true, 0);
    vbox.pack_start(&bbox, false, false, 0);

    win.connect_delete_event(|_, _| { gtk::main_quit(); Inhibit(false) });
    win.show_all();
}

// =========================================================================
// 3. UNIFIED UMU FORM (Used by RunExe & ManageShortcuts)
// =========================================================================
struct UmuForm {
    pub grid: gtk::Grid,
    pub exe_entry: gtk::Entry,
    pub cmb_proton: gtk::ComboBoxText,
    pub chk_gamemode: gtk::CheckButton,
    pub chk_mangohud: gtk::CheckButton,
    pub chk_wayland: gtk::CheckButton,
    pub chk_steam: gtk::CheckButton,
    pub chk_overlay: gtk::CheckButton,
    pub chk_vpn: gtk::CheckButton,
    pub prefix_entry: gtk::Entry,
    pub gpu_combo: gtk::ComboBoxText,
    pub name_entry: gtk::Entry,
    pub icon_chooser: gtk::FileChooserButton,
    pub args_entry: gtk::Entry,
    pub gameid_entry: gtk::Entry,
    pub img_preview: gtk::Image,
}

impl UmuForm {
    fn new(win: &gtk::Window) -> Self {
        let grid = gtk::Grid::new();
        grid.set_column_spacing(15);
        grid.set_row_spacing(10);

        let exe_entry = gtk::Entry::new();
        let cmb_proton = gtk::ComboBoxText::new();
        let chk_gamemode = gtk::CheckButton::new();
        let chk_mangohud = gtk::CheckButton::new();
        let chk_wayland = gtk::CheckButton::new();
        let chk_steam = gtk::CheckButton::new();
        let chk_overlay = gtk::CheckButton::new();
        let chk_vpn = gtk::CheckButton::new();
        let prefix_entry = gtk::Entry::new();
        let gpu_combo = gtk::ComboBoxText::new();
        let name_entry = gtk::Entry::new();
        let icon_chooser = gtk::FileChooserButton::new("Выберите иконку", gtk::FileChooserAction::Open);
        let args_entry = gtk::Entry::new();
        let gameid_entry = gtk::Entry::new();
        let img_preview = gtk::Image::from_icon_name(Some("wine"), gtk::IconSize::Dialog);

        // Load Proton Versions from Env
        let p_json = env::var("UMU_PROTON_VERSIONS_JSON").unwrap_or_default();
        if let Ok(versions) = serde_json::from_str::<serde_json::Value>(&p_json) {
            if let Some(arr) = versions.as_array() {
                for v in arr {
                    if let Some(n) = v.get("name").and_then(|n| n.as_str()) {
                        cmb_proton.append(Some(n), n);
                    }
                }
            }
        }
        cmb_proton.set_active(Some(0));

        ["Автоматически", "AMD", "Nvidia", "Intel"].iter().for_each(|g| gpu_combo.append_text(g));

        let add_row = |row: i32, label: &str, widget: &gtk::Widget| {
            let lbl = gtk::Label::builder().label(label).xalign(0.0).yalign(0.5).build();
            grid.attach(&lbl, 0, row, 1, 1);
            grid.attach(widget, 1, row, 1, 1);
        };

        // Browse EXE Box
        let exe_hbox = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        exe_hbox.pack_start(&exe_entry, true, true, 0);
        let btn_exe_browse = gtk::Button::with_label("Обзор...");
        let w_clone = win.clone();
        let e_clone = exe_entry.clone();
        btn_exe_browse.connect_clicked(move |_| {
            let dlg = gtk::FileChooserDialog::new(Some("Выберите .exe"), Some(&w_clone), gtk::FileChooserAction::Open);
            dlg.add_buttons(&[("Отмена", gtk::ResponseType::Cancel), ("Ок", gtk::ResponseType::Ok)]);
            if dlg.run() == gtk::ResponseType::Ok {
                if let Some(f) = dlg.filename() { e_clone.set_text(&f.to_string_lossy()); }
            }
            dlg.destroy();
        });
        exe_hbox.pack_start(&btn_exe_browse, false, false, 0);

        add_row(0, "Файл (.exe):", &exe_hbox);
        add_row(1, "Версия Proton:", &cmb_proton);
        add_row(2, "GameMode:", &chk_gamemode);
        add_row(3, "MangoHud:", &chk_mangohud);
        add_row(4, "Wayland:", &chk_wayland);
        add_row(5, "Steam Интеграция:", &chk_steam);
        add_row(6, "Steam Оверлей:", &chk_overlay);
        add_row(7, "VPN:", &chk_vpn);
        add_row(8, "Префикс:", &prefix_entry);
        add_row(9, "Видеокарта:", &gpu_combo);
        add_row(10, "Название:", &name_entry);
        
        let icon_hbox = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        icon_hbox.pack_start(&icon_chooser, true, true, 0);
        
        // Steam search integration could go here
        
        add_row(11, "Иконка:", &icon_hbox);
        add_row(12, "Аргументы:", &args_entry);
        add_row(13, "App ID:", &gameid_entry);

        // Preview box
        let prev_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        prev_box.pack_start(&gtk::Label::new(Some("Предпросмотр:")), false, false, 0);
        prev_box.pack_start(&img_preview, false, false, 0);
        grid.attach(&prev_box, 0, 14, 2, 1);

        // Interactive Toggles
        let cw = chk_wayland.clone();
        chk_overlay.connect_toggled(move |c| if c.is_active() { cw.set_active(false); cw.set_sensitive(false); } else { cw.set_sensitive(true); });
        let co = chk_overlay.clone();
        chk_wayland.connect_toggled(move |c| if c.is_active() { co.set_active(false); co.set_sensitive(false); } else { co.set_sensitive(true); });

        Self {
            grid, exe_entry, cmb_proton, chk_gamemode, chk_mangohud, chk_wayland,
            chk_steam, chk_overlay, chk_vpn, prefix_entry, gpu_combo, name_entry,
            icon_chooser, args_entry, gameid_entry, img_preview
        }
    }

    fn populate_from_desktop(&self, path: &str) {
        self.name_entry.set_text(&read_desktop_prop(path, "Name").replace(" (Inactive)", ""));
        self.exe_entry.set_text(&read_desktop_prop(path, "X-UMU-Actual-Exe"));
        self.args_entry.set_text(&read_desktop_prop(path, "X-UMU-Raw-Args"));
        self.prefix_entry.set_text(&read_desktop_prop(path, "X-UMU-Prefix-Name"));
        self.gameid_entry.set_text(&read_desktop_prop(path, "X-UMU-Game-ID"));
        
        let gpu = read_desktop_prop(path, "X-UMU-GPU-Select");
        if !gpu.is_empty() { self.gpu_combo.set_active_id(Some(&gpu)); }
        
        let proton = read_desktop_prop(path, "X-UMU-Proton-Type");
        if !proton.is_empty() { self.cmb_proton.set_active_id(Some(&proton)); }

        self.chk_gamemode.set_active(read_desktop_prop(path, "Exec").contains("USE_GAMEMODE=1"));
        self.chk_mangohud.set_active(read_desktop_prop(path, "Exec").contains("USE_MANGOHUD=1"));
        self.chk_wayland.set_active(read_desktop_prop(path, "Exec").contains("PROTON_ENABLE_WAYLAND=1"));
        
        self.chk_steam.set_active(read_desktop_prop(path, "X-UMU-Steam-Integration") == "1");
        self.chk_overlay.set_active(read_desktop_prop(path, "X-UMU-Steam-Overlay") == "1");
        self.chk_vpn.set_active(read_desktop_prop(path, "X-UMU-VPN") == "1");

        if let Some(icon) = read_desktop_prop(path, "Icon").into() {
            self.icon_chooser.set_filename(&icon);
        }
    }

    fn build_env_and_exec(&self) -> (HashMap<String, String>, String) {
        let mut envs = HashMap::new();
        envs.insert("UMU_PROTON_TYPE".into(), self.cmb_proton.active_id().unwrap_or_default().to_string());
        envs.insert("USE_GAMEMODE".into(), if self.chk_gamemode.is_active() { "1".into() } else { "0".into() });
        envs.insert("USE_MANGOHUD".into(), if self.chk_mangohud.is_active() { "1".into() } else { "0".into() });
        envs.insert("PROTON_ENABLE_WAYLAND".into(), if self.chk_wayland.is_active() { "1".into() } else { "0".into() });
        envs.insert("USE_STEAM_INTEGRATION".into(), if self.chk_steam.is_active() { "1".into() } else { "0".into() });
        envs.insert("USE_STEAM_OVERLAY".into(), if self.chk_overlay.is_active() { "1".into() } else { "0".into() });
        envs.insert("USE_VPN".into(), if self.chk_vpn.is_active() { "1".into() } else { "0".into() });
        envs.insert("UMU_PREFIX_NAME".into(), self.prefix_entry.text().to_string());
        envs.insert("UMU_GPU_SELECT".into(), self.gpu_combo.active_text().unwrap_or_default().to_string());
        envs.insert("GAMEID".into(), self.gameid_entry.text().to_string());

        let exe = self.exe_entry.text().to_string();
        let args = self.args_entry.text().to_string();
        
        let env_str = format!("env GAMEID={} USE_GAMEMODE={} USE_MANGOHUD={} PROTON_ENABLE_WAYLAND={} UMU_PREFIX_NAME={} UMU_PROTON_TYPE=\"{}\" USE_STEAM_INTEGRATION={} USE_STEAM_OVERLAY={} USE_VPN={} UMU_GPU_SELECT=\"{}\"",
            envs["GAMEID"], envs["USE_GAMEMODE"], envs["USE_MANGOHUD"], envs["PROTON_ENABLE_WAYLAND"], envs["UMU_PREFIX_NAME"], envs["UMU_PROTON_TYPE"], envs["USE_STEAM_INTEGRATION"], envs["USE_STEAM_OVERLAY"], envs["USE_VPN"], envs["UMU_GPU_SELECT"]);

        let exec = if args.contains("%command%") {
            let parts: Vec<&str> = args.splitn(2, "%command%").collect();
            format!("{} {} umu-run-wrapper \"{}\" {}", env_str, parts[0], exe, parts.get(1).unwrap_or(&""))
        } else {
            format!("{} umu-run-wrapper \"{}\" {}", env_str, exe, args)
        };

        (envs, exec)
    }
}

// =========================================================================
// 4. RUN EXE
// =========================================================================
fn run_exe(filepath: &str) {
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_title("UMU Launcher");
    win.set_default_size(480, -1);
    win.set_position(gtk::WindowPosition::Center);
    win.set_border_width(15);
    apply_floating_hack(&win);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
    win.add(&vbox);

    let form = UmuForm::new(&win);
    vbox.pack_start(&form.grid, false, false, 0);

    form.exe_entry.set_text(filepath);
    form.prefix_entry.set_text(&env::var("UMU_PREFIX_NAME").unwrap_or_else(|_| "default".to_string()));
    form.chk_gamemode.set_active(env::var("USE_GAMEMODE").unwrap_or_else(|_| "1".to_string()) != "0");
    form.chk_mangohud.set_active(env::var("USE_MANGOHUD").unwrap_or_else(|_| "1".to_string()) != "0");
    form.chk_wayland.set_active(env::var("PROTON_ENABLE_WAYLAND").unwrap_or_else(|_| "1".to_string()) != "0");
    form.chk_steam.set_active(env::var("USE_STEAM_INTEGRATION").unwrap_or_else(|_| "0".to_string()) == "1");
    form.chk_overlay.set_active(env::var("USE_STEAM_OVERLAY").unwrap_or_else(|_| "0".to_string()) == "1");
    form.chk_vpn.set_active(env::var("USE_VPN").unwrap_or_else(|_| "0".to_string()) == "1");
    
    // Auto-extract args if .lnk
    if filepath.to_lowercase().ends_with(".lnk") {
        if let Ok(out) = Command::new("exiftool").args(["-s3", "-CommandLineArguments", filepath]).output() {
            let extracted = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if extracted != "-" { form.args_entry.set_text(&extracted); }
        }
        let fname = std::path::Path::new(filepath).file_name().unwrap_or_default().to_string_lossy().replace(".lnk", "");
        form.name_entry.set_text(&fname);
    } else {
        let fname = std::path::Path::new(filepath).file_name().unwrap_or_default().to_string_lossy().replace(".exe", "");
        form.name_entry.set_text(&fname);
    }

    // Auto extract icon using fast shell script
    let exe_clone = filepath.to_string();
    std::thread::spawn(move || {
        let script = format!("wrestool -x -t 14 '{}' > /tmp/i.ico 2>/dev/null; magick /tmp/i.ico /tmp/i.png 2>/dev/null; ls -S /tmp/i*.png 2>/dev/null | head -n1", exe_clone);
        if let Ok(out) = Command::new("sh").arg("-c").arg(&script).output() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                glib::idle_add_local_once(move || {
                    // We don't have direct access to icon chooser here without Rc<RefCell>, skipping auto-set to keep small.
                    // Or we just don't auto-set to keep the binary small.
                });
            }
        }
    });

    let bbox = gtk::ButtonBox::new(gtk::Orientation::Horizontal);
    bbox.set_layout(gtk::ButtonBoxStyle::End);
    bbox.set_spacing(10);
    vbox.pack_start(&bbox, false, false, 0);

    let btn_run = gtk::Button::with_label("Запустить");
    let btn_create = gtk::Button::with_label("Создать .desktop");

    btn_run.connect_clicked(move |_| {
        let (envs, exec) = form.build_env_and_exec();
        Command::new("sh")
            .arg("-c")
            .arg(format!("{}; scan-umu-for-lnk", exec))
            .envs(&envs)
            .spawn().unwrap();
        gtk::main_quit();
    });

    btn_create.connect_clicked(move |_| {
        // Run create-desktop-with-umu bash script provided by Nix
        let (envs, _) = form.build_env_and_exec();
        Command::new("create-desktop-with-umu")
            .arg(form.exe_entry.text().to_string())
            .arg(if form.exe_entry.text().to_lowercase().ends_with(".lnk") { form.exe_entry.text().to_string() } else { "".to_string() })
            .arg(form.args_entry.text().to_string())
            .arg(form.name_entry.text().to_string())
            .arg(if let Some(i) = form.icon_chooser.filename() { i.to_string_lossy().to_string() } else { "wine".to_string() })
            .envs(&envs)
            .spawn().unwrap();
        gtk::main_quit();
    });

    bbox.pack_start(&btn_run, true, true, 0);
    bbox.pack_start(&btn_create, true, true, 0);

    win.connect_delete_event(|_, _| { gtk::main_quit(); Inhibit(false) });
    win.show_all();
}

// =========================================================================
// 5. MANAGE SHORTCUTS
// =========================================================================
fn manage_shortcuts() {
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_title("Manage UMU Shortcuts");
    win.set_default_size(750, 480);
    win.set_position(gtk::WindowPosition::Center);
    win.set_border_width(10);
    apply_floating_hack(&win);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    win.add(&vbox);

    let store = gtk::ListStore::new(&[glib::Type::STRING, glib::Type::STRING]);
    let treeview = gtk::TreeView::with_model(&store);
    let col = gtk::TreeViewColumn::new();
    col.set_title("Ярлыки");
    let renderer = gtk::CellRendererText::new();
    col.pack_start(&renderer, true);
    col.add_attribute(&renderer, "text", 0);
    treeview.append_column(&col);

    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.add(&treeview);
    vbox.pack_start(&scroll, true, true, 0);

    let desk_dir = format!("{}/.local/share/applications", env::var("HOME").unwrap_or_default());
    let populate = {
        let store = store.clone();
        let desk_dir = desk_dir.clone();
        move || {
            store.clear();
            if let Ok(entries) = std::fs::read_dir(&desk_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with("umu-") && name.ends_with(".desktop") {
                        let path = entry.path().to_string_lossy().into_owned();
                        let dname = read_desktop_prop(&path, "Name");
                        store.insert_with_values(None, &[(0, &dname), (1, &path)]);
                    }
                }
            }
        }
    };
    populate();

    let bbox = gtk::ButtonBox::new(gtk::Orientation::Horizontal);
    bbox.set_layout(gtk::ButtonBoxStyle::End);

    let btn_edit = gtk::Button::with_label("Редактировать");
    let btn_del = gtk::Button::with_label("Удалить");

    let get_selected = {
        let treeview = treeview.clone();
        let store = store.clone();
        move || -> Option<(String, String)> {
            if let Some((_, iter)) = treeview.selection().selected() {
                Some((store.value(&iter, 0).get().unwrap(), store.value(&iter, 1).get().unwrap()))
            } else { None }
        }
    };

    let w_clone = win.clone();
    let gs1 = get_selected.clone();
    btn_edit.connect_clicked(move |_| {
        if let Some((_, path)) = gs1() {
            let dlg = gtk::Dialog::with_buttons(Some("Редактировать"), Some(&w_clone), gtk::DialogFlags::MODAL, &[("Отмена", gtk::ResponseType::Cancel), ("Сохранить", gtk::ResponseType::Ok)]);
            apply_floating_hack(dlg.upcast_ref());
            let content = dlg.content_area();
            let form = UmuForm::new(dlg.upcast_ref());
            content.pack_start(&form.grid, true, true, 10);
            form.populate_from_desktop(&path);
            dlg.show_all();

            if dlg.run() == gtk::ResponseType::Ok {
                let (_, exec) = form.build_env_and_exec();
                write_desktop_props(&path, &[
                    ("Name", &form.name_entry.text().to_string()),
                    ("X-UMU-Actual-Exe", &form.exe_entry.text().to_string()),
                    ("X-UMU-Raw-Args", &form.args_entry.text().to_string()),
                    ("X-UMU-Prefix-Name", &form.prefix_entry.text().to_string()),
                    ("X-UMU-Game-ID", &form.gameid_entry.text().to_string()),
                    ("X-UMU-GPU-Select", &form.gpu_combo.active_text().unwrap_or_default().to_string()),
                    ("X-UMU-Proton-Type", &form.cmb_proton.active_id().unwrap_or_default().to_string()),
                    ("X-UMU-Steam-Integration", if form.chk_steam.is_active() { "1" } else { "0" }),
                    ("X-UMU-Steam-Overlay", if form.chk_overlay.is_active() { "1" } else { "0" }),
                    ("X-UMU-VPN", if form.chk_vpn.is_active() { "1" } else { "0" }),
                    ("Exec", &exec),
                    ("Icon", &if let Some(i) = form.icon_chooser.filename() { i.to_string_lossy().to_string() } else { "wine".to_string() }),
                ]);
            }
            dlg.destroy();
        }
    });

    let w_clone2 = win.clone();
    let gs2 = get_selected.clone();
    btn_del.connect_clicked(move |_| {
        if let Some((name, path)) = gs2() {
            let dlg = gtk::MessageDialog::new(Some(&w_clone2), gtk::DialogFlags::MODAL, gtk::MessageType::Question, gtk::ButtonsType::YesNo, &format!("Удалить ярлык {}?", name));
            if dlg.run() == gtk::ResponseType::Yes {
                std::fs::remove_file(path).ok();
                populate();
            }
            dlg.destroy();
        }
    });

    bbox.pack_start(&btn_edit, true, true, 0);
    bbox.pack_start(&btn_del, true, true, 0);
    vbox.pack_start(&bbox, false, false, 0);

    win.connect_delete_event(|_, _| { gtk::main_quit(); Inhibit(false) });
    win.show_all();
}
