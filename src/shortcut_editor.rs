use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use crate::{
    FileChooserButton, PathsListWidget, extract_default_icon_for_exe, get_default_proton_name,
    get_xdg_data_home, load_proton_versions, open_steam_search_dialog, open_zoom_preview,
    persist_icon, read_desktop_prop, set_image_from_path_or_theme,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct PerfToggles {
    pub gamemode: bool,
    pub mangohud: bool,
}

#[derive(Clone, Debug)]
pub struct DisplayToggles {
    pub wayland: bool,
    pub overlay: bool,
    pub xwayland_mode: String,
}

impl Default for DisplayToggles {
    fn default() -> Self {
        Self {
            wayland: true,
            overlay: false,
            xwayland_mode: "passthrough".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SteamToggles {
    pub steam: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IsolationToggles {
    pub vpn: bool,
    pub sandbox: bool,
    pub gamepad: bool,
}

#[derive(Clone, Debug, Default)]
pub struct UmuShortcutData {
    pub name: String,
    pub exe: String,
    pub icon: String,
    pub args: String,
    pub prefix: String,
    pub proton: String,
    pub gpu: String,
    pub gameid: String,
    pub perf: PerfToggles,
    pub display: DisplayToggles,
    pub steam: SteamToggles,
    pub isolation: IsolationToggles,
    pub network_mode: String,
    pub extra_paths: String,
    pub lnk_path: String,
}

#[must_use]
pub fn load_shortcut_from_desktop(desktop_path: &str) -> UmuShortcutData {
    let current_name = read_desktop_prop(desktop_path, "Name");
    let current_icon = read_desktop_prop(desktop_path, "Icon");
    let actual_exe = read_desktop_prop(desktop_path, "X-UMU-Actual-Exe");
    let raw_args = read_desktop_prop(desktop_path, "X-UMU-Raw-Args");
    let prefix_name = read_desktop_prop(desktop_path, "X-UMU-Prefix-Name");
    let gpu_select = read_desktop_prop(desktop_path, "X-UMU-GPU-Select");
    let proton_type = read_desktop_prop(desktop_path, "X-UMU-Proton-Type");
    let gameid = read_desktop_prop(desktop_path, "X-UMU-Game-ID");
    let exec_line = read_desktop_prop(desktop_path, "Exec");
    let extra_paths = read_desktop_prop(desktop_path, "X-UMU-Extra-Paths");
    let lnk_path = read_desktop_prop(desktop_path, "X-UMU-Lnk-Path");

    let prop_or_exec = |key: &str, exec_match: &str, default_val: bool| {
        let p = read_desktop_prop(desktop_path, key);
        if !p.is_empty() {
            p != "0"
        } else if default_val {
            !exec_line.contains(&format!("{exec_match}=0"))
        } else {
            exec_line.contains(&format!("{exec_match}=1"))
        }
    };

    let gamemode = prop_or_exec("X-UMU-Gamemode", "USE_GAMEMODE", false);
    let mangohud = prop_or_exec("X-UMU-Mangohud", "USE_MANGOHUD", false);
    let wayland = prop_or_exec("X-UMU-Wayland", "PROTON_ENABLE_WAYLAND", false);
    let steam_int = prop_or_exec("X-UMU-Steam-Integration", "USE_STEAM_INTEGRATION", false);
    let overlay = prop_or_exec("X-UMU-Steam-Overlay", "USE_STEAM_OVERLAY", false);
    let vpn = prop_or_exec("X-UMU-VPN", "USE_VPN", false);
    let sandbox = prop_or_exec("X-UMU-Sandbox", "USE_SANDBOX", false);
    let gamepad = prop_or_exec("X-UMU-Gamepad", "USE_GAMEPAD", false);

    let x11_prop = read_desktop_prop(desktop_path, "X-UMU-X11");
    let xwayland_mode = if x11_prop == "sandboxed" {
        "sandboxed".to_string()
    } else {
        "passthrough".to_string()
    };

    let net_prop = read_desktop_prop(desktop_path, "X-UMU-Network");
    let network_mode = if net_prop.is_empty() {
        if vpn {
            "singbox".to_string()
        } else {
            "sandboxed".to_string()
        }
    } else {
        match net_prop.as_str() {
            "0" | "off" => "off".to_string(),
            "1" | "passthrough" => "passthrough".to_string(),
            "singbox" => "singbox".to_string(),
            _ => "sandboxed".to_string(),
        }
    };

    UmuShortcutData {
        name: current_name,
        exe: actual_exe,
        icon: if current_icon.is_empty() {
            "wine".to_string()
        } else {
            current_icon
        },
        args: raw_args,
        prefix: if prefix_name.is_empty() {
            "default".to_string()
        } else {
            prefix_name
        },
        proton: proton_type,
        gpu: if gpu_select.is_empty() {
            "Автоматически".to_string()
        } else {
            gpu_select
        },
        gameid,
        perf: PerfToggles { gamemode, mangohud },
        display: DisplayToggles {
            wayland,
            overlay,
            xwayland_mode,
        },
        steam: SteamToggles { steam: steam_int },
        isolation: IsolationToggles {
            vpn,
            sandbox,
            gamepad,
        },
        network_mode,
        extra_paths,
        lnk_path,
    }
}

#[must_use]
pub fn resolve_actual_exe_from_lnk(target_exe: &str, prefix_name: &str) -> (String, String) {
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

/// Saves the UMU configuration into a `.desktop` file.
///
/// # Errors
///
/// Saves the UMU configuration into a `.desktop` file.
///
/// # Errors
///
/// Returns an [`io::Error`] if creating directories or writing the desktop entry fails.
pub fn save_shortcut_to_desktop(
    data: &UmuShortcutData,
    dest_path: Option<&str>,
) -> io::Result<PathBuf> {
    let desktop_dir = get_xdg_data_home().join("applications");
    fs::create_dir_all(&desktop_dir)?;
    let icon_dir = get_xdg_data_home().join("icons/umu");
    fs::create_dir_all(&icon_dir)?;

    let (actual_exe, lnk_path) = if data.exe.to_lowercase().ends_with(".lnk") {
        resolve_actual_exe_from_lnk(&data.exe, &data.prefix)
    } else {
        (data.exe.clone(), data.lnk_path.clone())
    };

    let digest = crate::md5::compute(format!("{}{}", actual_exe, data.args).as_bytes());
    let mut hash_hex = String::with_capacity(32);
    for byte in digest {
        let _ = write!(hash_hex, "{byte:02x}");
    }
    let hash_8 = hash_hex[..8].to_string();

    let target_desktop = dest_path.map_or_else(
        || desktop_dir.join(format!("umu-{hash_8}.desktop")),
        PathBuf::from,
    );

    let final_icon = if data.icon.is_empty() || data.icon == "wine" {
        "wine".to_string()
    } else {
        persist_icon(&data.icon)
    };

    let exe_dir = Path::new(&actual_exe)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_string_lossy()
        .to_string();

    let bool_str = |b: bool| if b { "1" } else { "0" };
    let use_steam_ports = data.steam.steam || data.display.overlay;
    let net_val = match data.network_mode.as_str() {
        "off" => "0",
        "passthrough" => "1",
        "singbox" => "singbox",
        _ => "sandboxed",
    };

    let exec_cmd = format!("umu-run-wrapper \"{}\"", target_desktop.to_string_lossy());

    let props = [
        ("Type", "Application"),
        ("Name", data.name.as_str()),
        ("Exec", exec_cmd.as_str()),
        ("Icon", final_icon.as_str()),
        ("Path", exe_dir.as_str()),
        ("Terminal", "false"),
        ("Categories", "Game;"),
        ("X-UMU-Actual-Exe", actual_exe.as_str()),
        ("X-UMU-Raw-Args", data.args.as_str()),
        ("X-UMU-Prefix-Name", data.prefix.as_str()),
        ("X-UMU-GPU-Select", data.gpu.as_str()),
        ("X-UMU-Gamemode", bool_str(data.perf.gamemode)),
        ("X-UMU-Mangohud", bool_str(data.perf.mangohud)),
        ("X-UMU-Wayland", bool_str(data.display.wayland)),
        ("X-UMU-X11", data.display.xwayland_mode.as_str()),
        ("X-UMU-Steam-Integration", bool_str(data.steam.steam)),
        ("X-UMU-Steam-Overlay", bool_str(data.display.overlay)),
        ("X-UMU-Steam-Ports", bool_str(use_steam_ports)),
        ("X-UMU-Proton-Type", data.proton.as_str()),
        ("X-UMU-VPN", bool_str(data.isolation.vpn)),
        ("X-UMU-Game-ID", data.gameid.as_str()),
        ("X-UMU-Sandbox", bool_str(data.isolation.sandbox)),
        ("X-UMU-Gamepad", bool_str(data.isolation.gamepad)),
        ("X-UMU-Network", net_val),
        ("X-UMU-Extra-Paths", data.extra_paths.as_str()),
        ("X-UMU-Lnk-Path", lnk_path.as_str()),
    ];

    let mut desktop_content = String::from("[Desktop Entry]\n");
    for (k, v) in &props {
        let _ = writeln!(desktop_content, "{k}={v}");
    }
    fs::write(&target_desktop, &desktop_content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&target_desktop, fs::Permissions::from_mode(0o755));
    }

    let _ = Command::new("notify-send")
        .args([
            "-i",
            &final_icon,
            "Ярлык сохранен",
            &format!("Ярлык для '{}' успешно сохранен", data.name),
        ])
        .spawn();

    Ok(target_desktop)
}

#[derive(Clone)]
pub struct UmuOptionsWidget {
    container: gtk4::Box,
    pub exe_entry: gtk4::Entry,
    pub cmb_proton: gtk4::ComboBoxText,
    pub cmb_gpu: gtk4::ComboBoxText,
    pub ent_prefix: gtk4::Entry,
    pub ent_name: gtk4::Entry,
    pub ent_args: gtk4::Entry,
    pub ent_gameid: gtk4::Entry,
    pub fc_icon: FileChooserButton,
    pub img_preview: gtk4::Image,
    pub chk_gamemode: gtk4::CheckButton,
    pub chk_mangohud: gtk4::CheckButton,
    pub chk_wayland: gtk4::CheckButton,
    pub cmb_xwayland: gtk4::ComboBoxText,
    pub chk_steam: gtk4::CheckButton,
    pub chk_overlay: gtk4::CheckButton,
    pub chk_vpn: gtk4::CheckButton,
    pub chk_sandbox: gtk4::CheckButton,
    pub cmb_network: gtk4::ComboBoxText,
    pub chk_gamepad: gtk4::CheckButton,
    pub paths_widget: PathsListWidget,
    pub lnk_path: Rc<RefCell<String>>,
}

fn build_steam_search_handler(
    parent: &gtk4::Window,
    ent_gameid: &gtk4::Entry,
    ent_name: &gtk4::Entry,
    fc_icon: &FileChooserButton,
    img_preview: &gtk4::Image,
) -> Rc<dyn Fn()> {
    let p_win = parent.clone();
    let gid_ent = ent_gameid.clone();
    let name_ent = ent_name.clone();
    let fc = fc_icon.clone();
    let img = img_preview.clone();

    Rc::new(move || {
        let game_id_entry = gid_ent.clone();
        let name = name_ent.clone();
        let fc_cb = fc.clone();
        let img_cb = img.clone();

        open_steam_search_dialog(&p_win, move |game_name, appid, icon_path| {
            game_id_entry.set_text(&appid);
            name.set_text(&game_name);
            if let Some(ref ico) = icon_path {
                fc_cb.set_filename(ico);
                set_image_from_path_or_theme(&img_cb, ico, 48);
            }
        });
    })
}

fn attach_grid_row(grid: &gtk4::Grid, row: i32, title: &str, widget: &gtk4::Widget) {
    let lbl = gtk4::Label::builder()
        .label(title)
        .xalign(0.0)
        .yalign(0.5)
        .build();
    grid.attach(&lbl, 0, row, 1, 1);
    grid.attach(widget, 1, row, 1, 1);
}

struct TogglesWidgets {
    gamemode: gtk4::CheckButton,
    mangohud: gtk4::CheckButton,
    wayland: gtk4::CheckButton,
    steam: gtk4::CheckButton,
    overlay: gtk4::CheckButton,
    vpn: gtk4::CheckButton,
    sandbox: gtk4::CheckButton,
}

fn build_main_toggles(grid: &gtk4::Grid, data: &UmuShortcutData) -> TogglesWidgets {
    let chk_gamemode = gtk4::CheckButton::new();
    chk_gamemode.set_active(data.perf.gamemode);
    attach_grid_row(grid, 9, "GameMode", chk_gamemode.upcast_ref());

    let chk_mangohud = gtk4::CheckButton::new();
    chk_mangohud.set_active(data.perf.mangohud);
    attach_grid_row(grid, 10, "MangoHud", chk_mangohud.upcast_ref());

    let chk_wayland = gtk4::CheckButton::new();
    chk_wayland.set_active(data.display.wayland);
    attach_grid_row(grid, 11, "Wayland", chk_wayland.upcast_ref());

    let chk_steam = gtk4::CheckButton::new();
    chk_steam.set_active(data.steam.steam);
    attach_grid_row(grid, 12, "Интегр. Steam", chk_steam.upcast_ref());

    let chk_overlay = gtk4::CheckButton::new();
    chk_overlay.set_active(data.display.overlay);
    attach_grid_row(grid, 13, "Оверлей Steam", chk_overlay.upcast_ref());

    let chk_vpn = gtk4::CheckButton::new();
    chk_vpn.set_active(data.isolation.vpn);
    attach_grid_row(grid, 14, "Через VPN", chk_vpn.upcast_ref());

    let chk_sandbox = gtk4::CheckButton::new();
    chk_sandbox.set_active(data.isolation.sandbox);
    attach_grid_row(grid, 15, "Песочница", chk_sandbox.upcast_ref());

    TogglesWidgets {
        gamemode: chk_gamemode,
        mangohud: chk_mangohud,
        wayland: chk_wayland,
        steam: chk_steam,
        overlay: chk_overlay,
        vpn: chk_vpn,
        sandbox: chk_sandbox,
    }
}

impl UmuOptionsWidget {
    #[must_use]
    pub fn new(parent_window: &gtk4::Window, initial_data: Option<&UmuShortcutData>) -> Self {
        let defaults = UmuShortcutData::default();
        let data = initial_data.unwrap_or(&defaults);

        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 15);
        let left_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        left_box.set_hexpand(true);

        let grid = gtk4::Grid::new();
        grid.set_column_spacing(15);
        grid.set_row_spacing(8);

        let paths_widget = PathsListWidget::new(parent_window);
        if !data.extra_paths.trim().is_empty() {
            paths_widget.load_from_string_args(&data.extra_paths);
        }

        let (exe_hbox, exe_entry) = Self::build_exe_row(parent_window, &data.exe);
        let cmb_proton = Self::build_proton_combo(&data.proton);
        let cmb_gpu = Self::build_gpu_combo(&data.gpu);

        let ent_prefix = gtk4::Entry::new();
        ent_prefix.set_text(if data.prefix.is_empty() {
            "default"
        } else {
            &data.prefix
        });

        let ent_name = gtk4::Entry::new();
        ent_name.set_text(&data.name);

        let (fc_icon, img_preview, icon_hbox, preview_hbox, btn_icon_search) =
            Self::build_icon_rows(parent_window, &data.icon, &exe_entry, &ent_prefix);

        let ent_args = gtk4::Entry::new();
        ent_args.set_text(&data.args);
        ent_args.set_placeholder_text(Some("VAR=1 %command% --dx11"));

        let (gameid_hbox, ent_gameid, btn_steam_search) = Self::build_gameid_row(&data.gameid);

        let search_handler = build_steam_search_handler(
            parent_window,
            &ent_gameid,
            &ent_name,
            &fc_icon,
            &img_preview,
        );
        let sh1 = Rc::clone(&search_handler);
        btn_steam_search.connect_clicked(move |_| sh1());
        let sh2 = Rc::clone(&search_handler);
        btn_icon_search.connect_clicked(move |_| sh2());

        attach_grid_row(&grid, 0, "Файл (.exe)", exe_hbox.upcast_ref());
        attach_grid_row(&grid, 1, "Версия Proton", cmb_proton.upcast_ref());
        attach_grid_row(&grid, 2, "Видеокарта", cmb_gpu.upcast_ref());
        attach_grid_row(&grid, 3, "Префикс (в ~/.umu/)", ent_prefix.upcast_ref());
        attach_grid_row(&grid, 4, "Название", ent_name.upcast_ref());
        attach_grid_row(&grid, 5, "Иконка (файл)", icon_hbox.upcast_ref());
        attach_grid_row(&grid, 6, "Предпросмотр иконки", preview_hbox.upcast_ref());
        attach_grid_row(&grid, 7, "Аргументы / Env", ent_args.upcast_ref());
        attach_grid_row(&grid, 8, "Game ID / App ID", gameid_hbox.upcast_ref());

        let toggles = build_main_toggles(&grid, data);

        left_box.append(&grid);

        let separator = gtk4::Separator::new(gtk4::Orientation::Vertical);
        separator.set_visible(data.isolation.sandbox);

        let (sandbox_pane, cmb_network, cmb_xwayland, chk_gamepad) =
            Self::build_sandbox_pane(data, &paths_widget);

        Self::connect_wayland_overlay_lock(
            &toggles.wayland,
            &toggles.overlay,
            &cmb_xwayland,
            data.display.overlay,
        );

        Self::connect_sandbox_expand(parent_window, &toggles.sandbox, &sandbox_pane, &separator);

        container.append(&left_box);
        container.append(&separator);
        container.append(&sandbox_pane);

        let lnk_path = Rc::new(RefCell::new(data.lnk_path.clone()));

        Self {
            container,
            exe_entry,
            cmb_proton,
            cmb_gpu,
            ent_prefix,
            ent_name,
            ent_args,
            ent_gameid,
            fc_icon,
            img_preview,
            chk_gamemode: toggles.gamemode,
            chk_mangohud: toggles.mangohud,
            chk_wayland: toggles.wayland,
            cmb_xwayland,
            chk_steam: toggles.steam,
            chk_overlay: toggles.overlay,
            chk_vpn: toggles.vpn,
            chk_sandbox: toggles.sandbox,
            cmb_network,
            chk_gamepad,
            paths_widget,
            lnk_path,
        }
    }

    fn build_exe_row(parent: &gtk4::Window, initial_exe: &str) -> (gtk4::Box, gtk4::Entry) {
        let exe_entry = gtk4::Entry::new();
        exe_entry.set_text(initial_exe);
        exe_entry.set_hexpand(true);
        let btn_exe_browse = gtk4::Button::with_label("Обзор...");
        let exe_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        exe_hbox.append(&exe_entry);
        exe_hbox.append(&btn_exe_browse);

        let w_browse = parent.clone();
        let ec_browse = exe_entry.clone();
        btn_exe_browse.connect_clicked(move |_| {
            let filter = gtk4::FileFilter::new();
            filter.set_name(Some("Исполняемые файлы (*.exe, *.lnk)"));
            filter.add_pattern("*.exe");
            filter.add_pattern("*.EXE");
            filter.add_pattern("*.lnk");
            filter.add_pattern("*.LNK");

            let dialog = gtk4::FileChooserNative::new(
                Some("Выберите исполняемый файл"),
                Some(&w_browse),
                gtk4::FileChooserAction::Open,
                Some("Выбрать"),
                Some("Отмена"),
            );
            dialog.add_filter(&filter);
            let ec = ec_browse.clone();
            dialog.connect_response(move |d, res| {
                if res == gtk4::ResponseType::Accept
                    && let Some(f) = d.file()
                    && let Some(p) = f.path()
                {
                    ec.set_text(&p.to_string_lossy());
                }
                d.destroy();
            });
            dialog.show();
        });

        (exe_hbox, exe_entry)
    }

    fn build_proton_combo(current_proton: &str) -> gtk4::ComboBoxText {
        let proton_versions = load_proton_versions();
        let cmb_proton = gtk4::ComboBoxText::new();
        let default_proton = get_default_proton_name(&proton_versions);
        let active_proton = if current_proton.is_empty() {
            default_proton
        } else {
            current_proton.to_string()
        };
        for v in &proton_versions {
            cmb_proton.append(Some(&v.name), &v.name);
        }
        if !cmb_proton.set_active_id(Some(&active_proton)) {
            cmb_proton.set_active(Some(0));
        }
        cmb_proton
    }

    fn build_gpu_combo(current_gpu: &str) -> gtk4::ComboBoxText {
        let cmb_gpu = gtk4::ComboBoxText::new();
        for g in &["Автоматически", "AMD", "Intel", "Nvidia"] {
            cmb_gpu.append(Some(g), g);
        }
        if !cmb_gpu.set_active_id(Some(current_gpu)) {
            cmb_gpu.set_active(Some(0));
        }
        cmb_gpu
    }

    fn build_icon_rows(
        parent: &gtk4::Window,
        current_icon: &str,
        exe_entry: &gtk4::Entry,
        ent_prefix: &gtk4::Entry,
    ) -> (
        FileChooserButton,
        gtk4::Image,
        gtk4::Box,
        gtk4::Box,
        gtk4::Button,
    ) {
        let img_preview = gtk4::Image::new();
        img_preview.set_pixel_size(48);
        img_preview.set_valign(gtk4::Align::Center);

        let ip_cb = img_preview.clone();
        let fc_icon = FileChooserButton::new("Выберите иконку", parent, move |path| {
            set_image_from_path_or_theme(&ip_cb, &path, 48);
        });
        fc_icon.set_filename(current_icon);
        set_image_from_path_or_theme(&img_preview, current_icon, 48);

        let btn_icon_search = gtk4::Button::with_label("Поиск в Steam");
        let btn_icon_reset = gtk4::Button::with_label("Сбросить");

        let icon_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        icon_hbox.append(fc_icon.widget());
        icon_hbox.append(&btn_icon_search);
        icon_hbox.append(&btn_icon_reset);

        let btn_zoom = gtk4::Button::with_label("Увеличить");
        let preview_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        preview_hbox.append(&img_preview);
        preview_hbox.append(&btn_zoom);

        let w_zm = parent.clone();
        let fc_z = fc_icon.clone();
        btn_zoom.connect_clicked(move |_| {
            open_zoom_preview(&w_zm, &fc_z.get_filename());
        });

        let fc_reset = fc_icon.clone();
        let img_reset = img_preview.clone();
        let exe_reset = exe_entry.clone();
        let pfx_reset = ent_prefix.clone();
        btn_icon_reset.connect_clicked(move |_| {
            let cur_exe = exe_reset.text().to_string();
            let target_icon = extract_default_icon_for_exe(&cur_exe, &pfx_reset.text())
                .unwrap_or_else(|| "wine".to_string());
            fc_reset.set_filename(&target_icon);
            set_image_from_path_or_theme(&img_reset, &target_icon, 48);
        });

        (
            fc_icon,
            img_preview,
            icon_hbox,
            preview_hbox,
            btn_icon_search,
        )
    }

    fn build_gameid_row(initial_gameid: &str) -> (gtk4::Box, gtk4::Entry, gtk4::Button) {
        let ent_gameid = gtk4::Entry::new();
        ent_gameid.set_text(initial_gameid);
        ent_gameid.set_placeholder_text(Some("Например: 292030 (AppID)"));
        ent_gameid.set_hexpand(true);

        let btn_steam_search = gtk4::Button::with_label("Поиск в Steam");
        let gameid_hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        gameid_hbox.append(&ent_gameid);
        gameid_hbox.append(&btn_steam_search);

        (gameid_hbox, ent_gameid, btn_steam_search)
    }

    fn connect_wayland_overlay_lock(
        chk_wayland: &gtk4::CheckButton,
        chk_overlay: &gtk4::CheckButton,
        cmb_xwayland: &gtk4::ComboBoxText,
        initial_overlay: bool,
    ) {
        let lock_sig = Rc::new(RefCell::new(false));
        {
            let ls = Rc::clone(&lock_sig);
            let w_chk = chk_wayland.clone();
            chk_overlay.connect_toggled(move |btn| {
                if *ls.borrow() {
                    return;
                }
                *ls.borrow_mut() = true;
                if btn.is_active() {
                    w_chk.set_active(false);
                    w_chk.set_sensitive(false);
                } else {
                    w_chk.set_sensitive(true);
                }
                *ls.borrow_mut() = false;
            });
        }
        {
            let ls = Rc::clone(&lock_sig);
            let o_chk = chk_overlay.clone();
            let cmb_x = cmb_xwayland.clone();
            chk_wayland.connect_toggled(move |btn| {
                let active = btn.is_active();
                cmb_x.set_sensitive(!active);
                if *ls.borrow() {
                    return;
                }
                *ls.borrow_mut() = true;
                if active {
                    o_chk.set_active(false);
                }
                *ls.borrow_mut() = false;
            });
        }
        if initial_overlay {
            chk_wayland.set_active(false);
            chk_wayland.set_sensitive(false);
        }
    }

    fn build_sandbox_pane(
        data: &UmuShortcutData,
        paths_widget: &PathsListWidget,
    ) -> (
        gtk4::Box,
        gtk4::ComboBoxText,
        gtk4::ComboBoxText,
        gtk4::CheckButton,
    ) {
        let sandbox_pane = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        sandbox_pane.set_size_request(380, -1);
        sandbox_pane.set_vexpand(true);
        sandbox_pane.set_visible(data.isolation.sandbox);

        let sb_grid = gtk4::Grid::new();
        sb_grid.set_column_spacing(12);
        sb_grid.set_row_spacing(8);

        let cmb_network = gtk4::ComboBoxText::new();
        cmb_network.append(Some("sandboxed"), "Изолированная (User-mode pasta)");
        cmb_network.append(Some("singbox"), "VPN (Sing-box TUN)");
        cmb_network.append(Some("passthrough"), "Прямой доступ (Passthrough)");
        cmb_network.append(Some("off"), "Отключена (Без сети)");
        if !cmb_network.set_active_id(Some(&data.network_mode)) {
            cmb_network.set_active(Some(0));
        }
        attach_grid_row(&sb_grid, 0, "Режим сети", cmb_network.upcast_ref());

        let cmb_xwayland = gtk4::ComboBoxText::new();
        cmb_xwayland.append(Some("passthrough"), "Прямой доступ (Хост X11)");
        cmb_xwayland.append(Some("sandboxed"), "Изолированный (xwayland-satellite)");
        if !cmb_xwayland.set_active_id(Some(&data.display.xwayland_mode)) {
            cmb_xwayland.set_active(Some(0));
        }
        cmb_xwayland.set_sensitive(!data.display.wayland);
        attach_grid_row(&sb_grid, 1, "XWayland (X11)", cmb_xwayland.upcast_ref());

        let chk_gamepad = gtk4::CheckButton::new();
        chk_gamepad.set_active(data.isolation.gamepad);
        attach_grid_row(
            &sb_grid,
            2,
            "Геймпад (/dev/input)",
            chk_gamepad.upcast_ref(),
        );

        sandbox_pane.append(&sb_grid);

        let pw_widget = paths_widget.widget();
        pw_widget.set_vexpand(true);
        sandbox_pane.append(pw_widget);

        (sandbox_pane, cmb_network, cmb_xwayland, chk_gamepad)
    }

    fn connect_sandbox_expand(
        parent: &gtk4::Window,
        chk_sandbox: &gtk4::CheckButton,
        sandbox_pane: &gtk4::Box,
        separator: &gtk4::Separator,
    ) {
        let p_win = parent.clone();
        let s_pane = sandbox_pane.clone();
        let sep = separator.clone();
        chk_sandbox.connect_toggled(move |btn| {
            let active = btn.is_active();
            s_pane.set_visible(active);
            sep.set_visible(active);
            if active {
                p_win.set_default_size(960, -1);
            } else {
                p_win.set_default_size(560, -1);
            }
        });
    }

    #[must_use]
    pub const fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    #[must_use]
    pub fn get_data(&self) -> UmuShortcutData {
        let name = self.ent_name.text().to_string();
        let exe = self.exe_entry.text().to_string();
        let icon = self.fc_icon.get_filename();
        let args = self.ent_args.text().to_string();
        let prefix = self.ent_prefix.text().to_string();
        let proton = self
            .cmb_proton
            .active_text()
            .unwrap_or_default()
            .to_string();
        let gpu = self.cmb_gpu.active_text().unwrap_or_default().to_string();
        let gameid = self.ent_gameid.text().to_string();

        let network_mode = self
            .cmb_network
            .active_id()
            .unwrap_or_else(|| "sandboxed".into())
            .to_string();
        let xwayland_mode = self
            .cmb_xwayland
            .active_id()
            .unwrap_or_else(|| "passthrough".into())
            .to_string();
        let extra_paths = self.paths_widget.to_string_args();
        let lnk_path = self.lnk_path.borrow().clone();

        UmuShortcutData {
            name,
            exe,
            icon,
            args,
            prefix: if prefix.is_empty() {
                "default".to_string()
            } else {
                prefix
            },
            proton,
            gpu: if gpu.is_empty() {
                "Автоматически".to_string()
            } else {
                gpu
            },
            gameid,
            perf: PerfToggles {
                gamemode: self.chk_gamemode.is_active(),
                mangohud: self.chk_mangohud.is_active(),
            },
            display: DisplayToggles {
                wayland: self.chk_wayland.is_active(),
                overlay: self.chk_overlay.is_active(),
                xwayland_mode,
            },
            steam: SteamToggles {
                steam: self.chk_steam.is_active(),
            },
            isolation: IsolationToggles {
                vpn: self.chk_vpn.is_active(),
                sandbox: self.chk_sandbox.is_active(),
                gamepad: self.chk_gamepad.is_active(),
            },
            network_mode,
            extra_paths,
            lnk_path,
        }
    }

    #[must_use]
    pub fn build_launch_envs(&self) -> HashMap<String, String> {
        let data = self.get_data();
        let mut envs = HashMap::new();
        envs.insert("UMU_PROTON_TYPE".into(), data.proton);
        envs.insert("UMU_GPU_SELECT".into(), data.gpu);
        envs.insert("UMU_PREFIX_NAME".into(), data.prefix);
        envs.insert("GAMEID".into(), data.gameid);

        let bool_str = |b: bool| if b { "1" } else { "0" };
        envs.insert("USE_GAMEMODE".into(), bool_str(data.perf.gamemode).into());
        envs.insert("USE_MANGOHUD".into(), bool_str(data.perf.mangohud).into());
        envs.insert(
            "PROTON_ENABLE_WAYLAND".into(),
            bool_str(data.display.wayland).into(),
        );
        envs.insert("UMU_X11_MODE".into(), data.display.xwayland_mode);
        envs.insert(
            "USE_STEAM_INTEGRATION".into(),
            bool_str(data.steam.steam).into(),
        );
        envs.insert(
            "USE_STEAM_OVERLAY".into(),
            bool_str(data.display.overlay).into(),
        );
        envs.insert("USE_VPN".into(), bool_str(data.isolation.vpn).into());
        envs.insert(
            "USE_SANDBOX".into(),
            bool_str(data.isolation.sandbox).into(),
        );
        envs.insert(
            "USE_GAMEPAD".into(),
            bool_str(data.isolation.gamepad).into(),
        );

        let net_val = match data.network_mode.as_str() {
            "off" => "0",
            "passthrough" => "1",
            "singbox" => "singbox",
            _ => "sandboxed",
        };
        envs.insert("USE_NETWORK".into(), net_val.into());
        let use_steam_ports = data.steam.steam || data.display.overlay;
        envs.insert("USE_STEAM_PORTS".into(), bool_str(use_steam_ports).into());

        if !data.extra_paths.is_empty() {
            envs.insert("UMU_EXTRA_PATHS".into(), data.extra_paths);
        }

        envs
    }
}
