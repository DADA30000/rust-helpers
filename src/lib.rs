use gdk_pixbuf::Pixbuf;
use gtk4::gdk;
use gtk4::pango;
use gtk4::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::thread;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Option<Self> {
        let unique = format!(
            "umu-tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let p = env::temp_dir().join(unique);
        fs::create_dir_all(&p).ok()?;
        Some(TempDir(p))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

pub fn apply_clean_theme() {
    let css = "
        listbox, listbox row {
            outline: none;
            box-shadow: none;
        }
        listbox row:focus, listbox row:focus-visible, listbox row:focus-within, listbox row:selected, listbox row:hover {
            outline: none;
            box-shadow: none;
        }
        image {
            -gtk-icon-style: requested;
        }
    ";
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(css);
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

#[derive(Clone)]
pub struct FileChooserButton {
    button: gtk4::Button,
    label: gtk4::Label,
    current_path: Rc<RefCell<String>>,
}

impl FileChooserButton {
    pub fn new(title: &str, parent: &gtk4::Window, on_file_set: impl Fn(String) + 'static) -> Self {
        let button = gtk4::Button::new();
        button.set_hexpand(true);

        let box_widget = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        box_widget.set_margin_start(4);
        box_widget.set_margin_end(4);

        let icon = gtk4::Image::from_icon_name("folder-symbolic");
        icon.set_valign(gtk4::Align::Center);

        let label = gtk4::Label::builder()
            .label("(Нет файла)")
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::Middle)
            .build();

        box_widget.append(&icon);
        box_widget.append(&label);
        button.set_child(Some(&box_widget));

        let current_path = Rc::new(RefCell::new(String::new()));
        let title_str = title.to_string();
        let parent_win = parent.clone();
        let cp_click = current_path.clone();
        let lbl_click = label.clone();
        let on_set = Rc::new(on_file_set);

        button.connect_clicked(move |_| {
            let dlg = gtk4::FileChooserNative::builder()
                .title(&title_str)
                .transient_for(&parent_win)
                .action(gtk4::FileChooserAction::Open)
                .build();

            let cp = cp_click.clone();
            let lbl = lbl_click.clone();
            let cb = on_set.clone();

            dlg.connect_response(move |d, res| {
                if res == gtk4::ResponseType::Accept {
                    if let Some(f) = d.file() {
                        let path = f.path().unwrap().to_string_lossy().to_string();
                        *cp.borrow_mut() = path.clone();
                        let fname = Path::new(&path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        lbl.set_text(&fname);
                        cb(path);
                    }
                }
                d.destroy();
            });
            dlg.show();
        });

        Self {
            button,
            label,
            current_path,
        }
    }

    pub fn widget(&self) -> &gtk4::Button {
        &self.button
    }

    pub fn get_filename(&self) -> String {
        self.current_path.borrow().clone()
    }

    pub fn set_filename(&self, path: &str) {
        *self.current_path.borrow_mut() = path.to_string();
        if path.is_empty() {
            self.label.set_text("(Нет файла)");
        } else {
            let fname = Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            self.label.set_text(&fname);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtonVersion {
    pub name: String,
    #[serde(default)]
    pub default: bool,
}

pub fn get_xdg_data_home() -> PathBuf {
    if let Ok(val) = env::var("XDG_DATA_HOME") {
        if !val.is_empty() {
            return PathBuf::from(val);
        }
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share")
}

pub fn get_cached_icons_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(home).join(".cache/umu/icons");
    fs::create_dir_all(&dir).ok();
    dir
}

pub fn load_proton_versions() -> Vec<ProtonVersion> {
    if let Ok(val) = env::var("UMU_PROTON_VERSIONS_JSON") {
        if let Ok(list) = serde_json::from_str::<Vec<ProtonVersion>>(&val) {
            if !list.is_empty() {
                return list;
            }
        }
    }

    let candidate = get_xdg_data_home().join("umu-ui/proton_versions.json");
    if candidate.exists() {
        if let Ok(content) = fs::read_to_string(candidate) {
            if let Ok(list) = serde_json::from_str::<Vec<ProtonVersion>>(&content) {
                if !list.is_empty() {
                    return list;
                }
            }
        }
    }

    vec![
        ProtonVersion {
            name: "Proton GE (Latest)".into(),
            default: false,
        },
        ProtonVersion {
            name: "Proton GE 10".into(),
            default: false,
        },
        ProtonVersion {
            name: "Proton UMU 10".into(),
            default: true,
        },
        ProtonVersion {
            name: "Proton UMU 9".into(),
            default: false,
        },
        ProtonVersion {
            name: "Proton UMU 8".into(),
            default: false,
        },
    ]
}

pub fn get_default_proton_name(versions: &[ProtonVersion]) -> String {
    versions
        .iter()
        .find(|v| v.default)
        .or_else(|| versions.first())
        .map(|v| v.name.clone())
        .unwrap_or_else(|| "Proton UMU 10".into())
}

pub fn get_games_appid_path() -> PathBuf {
    if let Ok(val) = env::var("STEAM_APP_ID_LIST_PATH") {
        let p = PathBuf::from(val);
        if p.exists() {
            return p;
        }
    }
    get_xdg_data_home().join("umu-ui/games_appid.json")
}

pub fn read_desktop_prop(path: &str, key: &str) -> String {
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    return v.to_string();
                }
            }
        }
    }
    String::new()
}

pub fn write_desktop_props(path: &str, props: &[(&str, &str)]) {
    if let Ok(content) = fs::read_to_string(path) {
        let mut new_lines = Vec::new();
        let mut written_keys = std::collections::HashSet::new();

        for line in content.lines() {
            let mut replaced = false;
            for (k, v) in props {
                if line.starts_with(&format!("{}=", k)) {
                    new_lines.push(format!("{}={}", k, v));
                    written_keys.insert(*k);
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                new_lines.push(line.to_string());
            }
        }

        for (k, v) in props {
            if !written_keys.contains(k) {
                new_lines.push(format!("{}={}", k, v));
            }
        }

        fs::write(path, new_lines.join("\n")).ok();
    }
}

pub fn set_image_from_path_or_theme(img: &gtk4::Image, path: &str, display_size: i32) {
    img.set_pixel_size(display_size);
    img.set_valign(gtk4::Align::Center);
    if !path.is_empty() && Path::new(path).exists() {
        let load_size = display_size * 2;
        if let Ok(pixbuf) = Pixbuf::from_file_at_scale(path, load_size, load_size, true) {
            let texture = gdk::Texture::for_pixbuf(&pixbuf);
            img.set_paintable(Some(&texture));
            return;
        }
    }
    img.set_icon_name(Some("wine"));
}

pub fn fetch_steam_icon_cached(appid: &str) -> Option<String> {
    let cache_dir = get_cached_icons_dir();
    let dest_path = cache_dir.join(format!("steam-{}.png", appid));
    if dest_path.exists() {
        return Some(dest_path.to_string_lossy().to_string());
    }

    let info_url = format!("https://api.steamcmd.net/v1/info/{}", appid);
    let resp: Value = ureq::get(&info_url)
        .timeout(std::time::Duration::from_secs(3))
        .call()
        .ok()?
        .into_json()
        .ok()?;

    let client_icon_hash = resp
        .get("data")?
        .get(appid)?
        .get("common")?
        .get("clienticon")?
        .as_str()?;

    let ico_url = format!(
        "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{}/{}.ico",
        appid, client_icon_hash
    );

    let mut ico_data = Vec::new();
    ureq::get(&ico_url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .ok()?
        .into_reader()
        .read_to_end(&mut ico_data)
        .ok()?;

    let tmp_dir = TempDir::new()?;
    let tmp_ico = tmp_dir.path().join("icon.ico");
    let tmp_png = tmp_dir.path().join("icon.png");
    fs::write(&tmp_ico, &ico_data).ok()?;

    Command::new("magick")
        .arg(&tmp_ico)
        .arg(&tmp_png)
        .output()
        .ok()?;

    let mut generated_pngs = Vec::new();
    if let Ok(entries) = fs::read_dir(tmp_dir.path()) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file()
                && p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with("icon")
                && p.extension().map_or(false, |ext| ext == "png")
            {
                if let Ok(meta) = p.metadata() {
                    generated_pngs.push((meta.len(), p));
                }
            }
        }
    }

    if generated_pngs.is_empty() {
        return None;
    }

    generated_pngs.sort_by(|a, b| b.0.cmp(&a.0));
    fs::copy(&generated_pngs[0].1, &dest_path).ok()?;

    Some(dest_path.to_string_lossy().to_string())
}

pub fn open_steam_search_dialog(
    parent: &gtk4::Window,
    on_selected: impl Fn(String, String, Option<String>) + 'static,
) {
    let win = gtk4::Window::builder()
        .title("Поиск в Steam")
        .default_width(550)
        .default_height(500)
        .transient_for(parent)
        .modal(true)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    vbox.set_margin_top(10);
    vbox.set_margin_bottom(10);
    vbox.set_margin_start(10);
    vbox.set_margin_end(10);
    win.set_child(Some(&vbox));

    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text("Введите название игры или AppID...")
        .build();
    vbox.append(&search_entry);

    let listbox = gtk4::ListBox::new();
    listbox.set_selection_mode(gtk4::SelectionMode::None);
    listbox.set_focusable(false);
    let scroll = gtk4::ScrolledWindow::builder()
        .child(&listbox)
        .vexpand(true)
        .build();
    vbox.append(&scroll);

    let status_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let lbl_status = gtk4::Label::new(Some("Загрузка базы данных..."));
    lbl_status.set_xalign(0.0);
    lbl_status.set_hexpand(true);
    let spinner = gtk4::Spinner::new();
    status_box.append(&lbl_status);
    status_box.append(&spinner);
    vbox.append(&status_box);

    let (db_tx, db_rx) = std::sync::mpsc::channel::<Vec<(String, String)>>();

    thread::spawn(move || {
        let db_path = get_games_appid_path();
        let mut apps = Vec::new();
        if let Ok(content) = fs::read_to_string(db_path) {
            if let Ok(data) = serde_json::from_str::<Value>(&content) {
                if let Some(arr) = data.as_array() {
                    for item in arr {
                        let name = item
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        let appid = item
                            .get("appid")
                            .map(|a| a.to_string().replace('"', ""))
                            .unwrap_or_default();
                        if !name.is_empty() && !appid.is_empty() {
                            apps.push((name, appid));
                        }
                    }
                } else if let Some(obj) = data.as_object() {
                    for (k, v) in obj {
                        let name = if let Some(n) = v.get("name").and_then(|n| n.as_str()) {
                            n.to_string()
                        } else if let Some(s) = v.as_str() {
                            s.to_string()
                        } else {
                            v.to_string()
                        };
                        if !name.is_empty() {
                            apps.push((name, k.clone()));
                        }
                    }
                }
            }
        }
        db_tx.send(apps).ok();
    });

    let apps_cache = Rc::new(RefCell::new(Vec::new()));
    let ac_load = apps_cache.clone();
    let lbl_load = lbl_status.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
        if let Ok(apps) = db_rx.try_recv() {
            if apps.is_empty() {
                lbl_load.set_markup("<span foreground='red'>БД Steam не найдена!</span>");
            } else {
                lbl_load.set_markup("<span foreground='green'>БД Steam загружена</span>");
                *ac_load.borrow_mut() = apps;
            }
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });

    let (icon_tx, icon_rx) = std::sync::mpsc::channel::<(String, String)>();
    let lb_icons = listbox.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
        while let Ok((appid, icon_path)) = icon_rx.try_recv() {
            let mut child = lb_icons.first_child();
            while let Some(row_widget) = child {
                if let Some(row) = row_widget.downcast_ref::<gtk4::ListBoxRow>() {
                    if row.widget_name().ends_with(&format!("|{}", appid)) {
                        if let Some(bx) = row.child().and_then(|c| c.downcast::<gtk4::Box>().ok()) {
                            if let Some(img_w) = bx.first_child() {
                                if let Some(img) = img_w.downcast_ref::<gtk4::Image>() {
                                    set_image_from_path_or_theme(img, &icon_path, 64);
                                }
                            }
                        }
                    }
                }
                child = row_widget.next_sibling();
            }
        }
        glib::ControlFlow::Continue
    });

    let ac_filter = apps_cache.clone();
    let lb_filter = listbox.clone();
    let lbl_search = lbl_status.clone();
    let spin_search = spinner.clone();

    search_entry.connect_search_changed(move |entry| {
        let query = entry.text().trim().to_lowercase();
        while let Some(child) = lb_filter.first_child() {
            lb_filter.remove(&child);
        }

        if query.is_empty() {
            lbl_search.set_markup("<span foreground='gray'>Ожидание поиска</span>");
            spin_search.stop();
            return;
        }

        let apps = ac_filter.borrow();
        let mut results = Vec::new();
        for (name, appid) in apps.iter() {
            if name.to_lowercase().contains(&query) || appid == &query {
                results.push((name.clone(), appid.clone()));
                if results.len() >= 80 {
                    break;
                }
            }
        }

        lbl_search.set_text(&format!("Найдено игр: {}", results.len()));

        for (name, appid) in &results {
            let row = gtk4::ListBoxRow::new();
            row.set_selectable(false);
            row.set_focusable(false);
            row.set_activatable(true);
            row.set_widget_name(&format!("{}|{}", name, appid));

            let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
            row_box.set_margin_top(6);
            row_box.set_margin_bottom(6);
            row_box.set_margin_start(8);
            row_box.set_margin_end(8);

            let img = gtk4::Image::new();
            let cache_file = get_cached_icons_dir().join(format!("steam-{}.png", appid));
            if cache_file.exists() {
                set_image_from_path_or_theme(&img, &cache_file.to_string_lossy().to_string(), 64);
            } else {
                set_image_from_path_or_theme(&img, "wine", 64);
                let a_clone = appid.clone();
                let itx = icon_tx.clone();
                thread::spawn(move || {
                    if let Some(path) = fetch_steam_icon_cached(&a_clone) {
                        itx.send((a_clone, path)).ok();
                    }
                });
            }

            let lbl = gtk4::Label::builder()
                .label(&format!("{} ({})", name, appid))
                .xalign(0.0)
                .hexpand(true)
                .build();

            row_box.append(&img);
            row_box.append(&lbl);
            row.set_child(Some(&row_box));
            lb_filter.append(&row);
        }
    });

    let on_sel = Rc::new(on_selected);
    let w_close = win.clone();
    listbox.connect_row_activated(move |_, row| {
        let name_id = row.widget_name().to_string();
        if let Some((name, appid)) = name_id.split_once('|') {
            let cache_file = get_cached_icons_dir().join(format!("steam-{}.png", appid));
            let icon_str = if cache_file.exists() {
                Some(cache_file.to_string_lossy().to_string())
            } else {
                None
            };
            on_sel(name.to_string(), appid.to_string(), icon_str);
            w_close.close();
        }
    });

    win.present();
}

pub fn persist_icon_as(icon_path: &str, custom_dest_filename: Option<&str>) -> String {
    if icon_path.is_empty() || icon_path == "wine" {
        return icon_path.to_string();
    }
    let path = Path::new(icon_path);
    if !path.exists() {
        return icon_path.to_string();
    }

    // Target directory: ~/.local/share/icons/umu
    let dest_dir = get_xdg_data_home().join("icons/umu");
    fs::create_dir_all(&dest_dir).ok();

    let dest_path = match custom_dest_filename {
        Some(name) => dest_dir.join(name),
        None => dest_dir.join(path.file_name().unwrap_or_default()),
    };

    // Copy if it's not already in the destination folder
    if path.canonicalize().unwrap_or_default() != dest_path.canonicalize().unwrap_or_default() {
        fs::copy(path, &dest_path).ok();
    }

    dest_path.to_string_lossy().to_string()
}

pub fn persist_icon(icon_path: &str) -> String {
    persist_icon_as(icon_path, None)
}

pub fn open_zoom_preview(parent: &gtk4::Window, icon_path: &str) {
    if icon_path.is_empty() || !Path::new(icon_path).exists() {
        return;
    }
    let zoom_win = gtk4::Window::builder()
        .title("Предпросмотр")
        .transient_for(parent)
        .modal(true)
        .default_width(300)
        .default_height(300)
        .build();

    let key_controller = gtk4::EventControllerKey::new();
    let zw = zoom_win.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk4::gdk::Key::Escape {
            zw.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    zoom_win.add_controller(key_controller);

    let img = gtk4::Image::new();
    set_image_from_path_or_theme(&img, icon_path, 256);
    img.set_margin_top(15);
    img.set_margin_bottom(15);
    img.set_margin_start(15);
    img.set_margin_end(15);
    zoom_win.set_child(Some(&img));
    zoom_win.present();
}

pub fn extract_default_icon_for_exe(filepath: &str, prefix_name: &str) -> Option<String> {
    let mut exe_target = filepath.to_string();
    if filepath.to_lowercase().ends_with(".lnk") {
        if let Ok(out) = Command::new("exiftool")
            .args(["-s3", "-LocalBasePath", filepath])
            .output()
        {
            let win_path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !win_path.is_empty() && win_path != "-" {
                let rel_path = regex_lite_sub_drive(&win_path);
                let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
                let pfx_dir = PathBuf::from(home).join(".umu").join(prefix_name);
                let rel_clean = rel_path.trim_start_matches('/');
                let p_upper = pfx_dir.join("upper/drive_c").join(rel_clean);
                let p_legacy = pfx_dir.join("drive_c").join(rel_clean);
                let p = if p_upper.exists() { p_upper } else { p_legacy };
                if p.exists() {
                    exe_target = p.to_string_lossy().to_string();
                }
            }
        }
    }

    let digest = md5::compute(filepath.as_bytes());
    let hash: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    let out_icon = get_cached_icons_dir().join(format!("umu-{}.png", &hash[..8]));
    if out_icon.exists() {
        return Some(out_icon.to_string_lossy().to_string());
    }

    let tmp_dir = TempDir::new()?;
    let tmp_ico = tmp_dir.path().join("icon.ico");
    let tmp_png = tmp_dir.path().join("icon.png");

    let mut has_ico = false;
    if exe_target.to_lowercase().ends_with(".ico") {
        if fs::copy(&exe_target, &tmp_ico).is_ok() {
            has_ico = true;
        }
    } else {
        if let Ok(res) = Command::new("wrestool")
            .args(["-x", "-t", "14", &exe_target])
            .output()
        {
            if !res.stdout.is_empty() && fs::write(&tmp_ico, res.stdout).is_ok() {
                has_ico = true;
            }
        }
        if !has_ico {
            if let Some(parent) = Path::new(&exe_target).parent() {
                let stem = Path::new(&exe_target)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                let stem_ico = parent.join(format!("{}.ico", stem));
                let stem_ico_lower = parent.join(format!("{}.ico", stem.to_lowercase()));
                if stem_ico.is_file() {
                    has_ico = fs::copy(&stem_ico, &tmp_ico).is_ok();
                } else if stem_ico_lower.is_file() {
                    has_ico = fs::copy(&stem_ico_lower, &tmp_ico).is_ok();
                } else if let Ok(entries) = fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_file()
                            && p.extension()
                                .map_or(false, |e| e.eq_ignore_ascii_case("ico"))
                        {
                            if fs::copy(&p, &tmp_ico).is_ok() {
                                has_ico = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    if has_ico {
        Command::new("magick")
            .arg(&tmp_ico)
            .arg(&tmp_png)
            .output()
            .ok();

        let mut generated = Vec::new();
        if let Ok(entries) = fs::read_dir(tmp_dir.path()) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .starts_with("icon")
                    && p.extension().map_or(false, |ext| ext == "png")
                {
                    if let Ok(m) = p.metadata() {
                        generated.push((m.len(), p));
                    }
                }
            }
        }
        if !generated.is_empty() {
            generated.sort_by(|a, b| b.0.cmp(&a.0));
            fs::copy(&generated[0].1, &out_icon).ok();
            return Some(out_icon.to_string_lossy().to_string());
        }
    }
    None
}

fn regex_lite_sub_drive(path: &str) -> String {
    let s = path.replace('\\', "/");
    if s.len() >= 2 && s.chars().nth(1) == Some(':') {
        s[2..].to_string()
    } else {
        s
    }
}

mod md5 {
    pub fn compute(input: &[u8]) -> [u8; 16] {
        let mut d = 0x10325476u32;
        let mut c = 0x98badcfeu32;
        let mut b = 0xefcdab89u32;
        let mut a = 0x67452301u32;
        let mut msg = input.to_vec();
        let orig_len = (input.len() as u64) * 8;
        msg.push(0x80);
        while (msg.len() % 64) != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&orig_len.to_le_bytes());

        for chunk in msg.chunks(64) {
            let mut w = [0u32; 16];
            for (i, c) in chunk.chunks(4).enumerate() {
                w[i] = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
            }
            let (mut aa, mut bb, mut cc, mut dd) = (a, b, c, d);

            let s = [
                7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14,
                20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11,
                16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
            ];
            let k: [u32; 64] = [
                0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
                0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
                0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
                0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
                0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
                0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
                0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
                0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
                0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
                0xeb86d391,
            ];

            for i in 0..64 {
                let (f, g) = match i {
                    0..=15 => ((bb & cc) | (!bb & dd), i),
                    16..=31 => ((dd & bb) | (!dd & cc), (5 * i + 1) % 16),
                    32..=47 => (bb ^ cc ^ dd, (3 * i + 5) % 16),
                    _ => (cc ^ (bb | !dd), (7 * i) % 16),
                };
                let temp = dd;
                dd = cc;
                cc = bb;
                bb = bb.wrapping_add(
                    (aa.wrapping_add(f).wrapping_add(k[i]).wrapping_add(w[g])).rotate_left(s[i]),
                );
                aa = temp;
            }
            a = a.wrapping_add(aa);
            b = b.wrapping_add(bb);
            c = c.wrapping_add(cc);
            d = d.wrapping_add(dd);
        }

        let mut res = [0u8; 16];
        res[0..4].copy_from_slice(&a.to_le_bytes());
        res[4..8].copy_from_slice(&b.to_le_bytes());
        res[8..12].copy_from_slice(&c.to_le_bytes());
        res[12..16].copy_from_slice(&d.to_le_bytes());
        res
    }
}
