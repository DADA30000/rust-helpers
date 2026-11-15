use gtk4::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use crate::{BindMode, PathsListWidget};

static EDIT_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct SbDefaults {
    pub network: String,
    pub wayland: String,
    pub x11: String,
    pub pulse: String,
    pub pipewire: String,
    pub dbus: String,
    pub shm: String,
    pub tmp: String,
    pub webcam: usize,
    pub hw: SandboxHwFlags,
    pub sec: SandboxSecurityFlags,
    pub runtime: SandboxRuntimeFlags,
}

#[derive(Clone, Debug, Deserialize)]
struct SbDefaultsRaw {
    network: String,
    wayland: String,
    x11: String,
    pulse: String,
    pipewire: String,
    dbus: String,
    shm: String,
    tmp: String,
    webcam: usize,
    #[serde(flatten)]
    flags: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SbSeccompDefaults {
    pub default_allowed: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct SbConfigFile {
    defaults: SbDefaultsRaw,
    bins: BTreeMap<String, String>,
    singbox_config: String,
    syscalls_file: String,
    seccomp: SbSeccompDefaults,
}

#[derive(Clone, Debug)]
pub struct SbRunConfig {
    pub defaults: SbDefaults,
    pub singbox_config: String,
    pub neovide_bin: String,
    pub default_allowed: Vec<String>,
    pub syscalls: BTreeSet<String>,
}

fn merge_json_values(base: &mut Value, overlay: Value) {
    if let (Value::Object(bmap), Value::Object(omap)) = (&mut *base, &overlay) {
        for (k, v) in omap {
            if let Some(existing) = bmap.get_mut(k) {
                merge_json_values(existing, v.clone());
            } else {
                bmap.insert(k.clone(), v.clone());
            }
        }
        return;
    }
    *base = overlay;
}

fn read_json_value(path: &Path) -> Result<Value, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("invalid JSON in '{}': {e}", path.display()))
}

fn load_merged_config_value() -> Result<Value, String> {
    let home = env::var("HOME").unwrap_or_default();
    let xdg_data_cfg = env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map_or_else(
            || {
                if home.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(&home).join(".local/share/sb-run/config.json"))
                }
            },
            |d| Some(PathBuf::from(d).join("sb-run/config.json")),
        );
    let sys_pb = PathBuf::from("/etc/sb-run/config.json");
    let explicit = env::var("SB_RUN_CONFIG")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    if let Some(ref ep) = explicit
        && !ep.is_file()
    {
        return Err(format!("SB_RUN_CONFIG '{}' does not exist", ep.display()));
    }

    let base_path = if sys_pb.is_file() {
        sys_pb
    } else if let Some(ref p) = xdg_data_cfg
        && p.is_file()
    {
        p.clone()
    } else if let Some(ref ep) = explicit {
        ep.clone()
    } else {
        return Err("missing declarative config (/etc/sb-run/config.json)".into());
    };

    let mut root = read_json_value(&base_path)?;

    let user_cfg = env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map_or_else(
            || {
                if home.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(&home).join(".config/sb-run/config.json"))
                }
            },
            |d| Some(PathBuf::from(d).join("sb-run/config.json")),
        );
    if let Some(ref u_path) = user_cfg
        && u_path.is_file()
        && *u_path != base_path
    {
        merge_json_values(&mut root, read_json_value(u_path)?);
    }
    if let Some(ref ep) = explicit
        && *ep != base_path
    {
        merge_json_values(&mut root, read_json_value(ep)?);
    }
    Ok(root)
}

impl SbRunConfig {
    /// Loads the declarative `sb-run` configuration and syscall table.
    ///
    /// # Errors
    ///
    /// Returns a descriptive error string if the config, syscall table, or required fields are missing.
    pub fn load() -> Result<Self, String> {
        let root_val = load_merged_config_value()?;
        let parsed: SbConfigFile = serde_json::from_value(root_val)
            .map_err(|e| format!("invalid sb-run config schema: {e}"))?;

        let neovide_bin = parsed
            .bins
            .get("neovide")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "missing 'bins.neovide' in sb-run config".to_string())?
            .clone();

        if !Path::new(&parsed.singbox_config).is_file() {
            return Err(format!(
                "missing default singbox_config '{}'",
                parsed.singbox_config
            ));
        }
        let sys_raw = fs::read_to_string(&parsed.syscalls_file)
            .map_err(|e| format!("missing syscalls_file '{}': {e}", parsed.syscalls_file))?;
        let sys_val: Value = serde_json::from_str(&sys_raw)
            .map_err(|e| format!("invalid syscalls JSON '{}': {e}", parsed.syscalls_file))?;

        let mut syscalls = BTreeSet::new();
        for arch in ["x86_64", "i386"] {
            if let Some(obj) = sys_val.get(arch).and_then(Value::as_object) {
                for key in obj.keys() {
                    syscalls.insert(key.clone());
                }
            }
        }
        if syscalls.is_empty() {
            return Err(format!(
                "syscalls_file '{}' contains no syscalls",
                parsed.syscalls_file
            ));
        }

        let flag_bool = |key: &str| -> Result<bool, String> {
            parsed
                .defaults
                .flags
                .get(key)
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("missing bool 'defaults.{key}' in sb-run config"))
        };
        let defaults = SbDefaults {
            network: parsed.defaults.network,
            wayland: parsed.defaults.wayland,
            x11: parsed.defaults.x11,
            pulse: parsed.defaults.pulse,
            pipewire: parsed.defaults.pipewire,
            dbus: parsed.defaults.dbus,
            shm: parsed.defaults.shm,
            tmp: parsed.defaults.tmp,
            webcam: parsed.defaults.webcam,
            hw: SandboxHwFlags {
                gpu: flag_bool("gpu")?,
                gamepad: flag_bool("gamepad")?,
            },
            sec: SandboxSecurityFlags {
                landlock: flag_bool("landlock")?,
                seccomp: flag_bool("seccomp")?,
                portals: flag_bool("portals")?,
            },
            runtime: SandboxRuntimeFlags {
                tmpfs: flag_bool("tmpfs")?,
                share_pid: flag_bool("share_pid")?,
                vpnify: flag_bool("vpnify")?,
            },
        };

        Ok(Self {
            defaults,
            singbox_config: parsed.singbox_config,
            neovide_bin,
            default_allowed: parsed.seccomp.default_allowed,
            syscalls,
        })
    }

    #[must_use]
    pub fn load_or_exit() -> Self {
        match Self::load() {
            Ok(cfg) => cfg,
            Err(err) => {
                eprintln!("[sb-gui] Error: {err}");
                std::process::exit(1);
            }
        }
    }

    #[must_use]
    pub fn is_valid_syscall(&self, name: &str) -> bool {
        matches!(name, "tiocsti" | "clone_newuser") || self.syscalls.contains(name)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SandboxHwFlags {
    pub gpu: bool,
    pub gamepad: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SandboxSecurityFlags {
    pub landlock: bool,
    pub seccomp: bool,
    pub portals: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SandboxRuntimeFlags {
    pub tmpfs: bool,
    pub share_pid: bool,
    pub vpnify: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxFormData {
    pub network: String,
    pub singbox_config: String,
    pub wayland: String,
    pub x11: String,
    pub pulse: String,
    pub pipewire: String,
    pub webcam: String,
    pub dbus: String,
    pub dbus_args: String,
    pub shm: String,
    pub tmp: String,
    pub hw: SandboxHwFlags,
    pub sec: SandboxSecurityFlags,
    pub runtime: SandboxRuntimeFlags,
    pub seccomp_allowed: Vec<String>,
    pub bridges: Vec<String>,
    pub bwrap_args: String,
    pub extra_env: String,
    pub extra_paths: String,
}

impl SandboxFormData {
    #[must_use]
    pub fn from_decl(decl: &SbRunConfig) -> Self {
        Self {
            network: decl.defaults.network.clone(),
            singbox_config: String::new(),
            wayland: decl.defaults.wayland.clone(),
            x11: decl.defaults.x11.clone(),
            pulse: decl.defaults.pulse.clone(),
            pipewire: decl.defaults.pipewire.clone(),
            webcam: decl.defaults.webcam.to_string(),
            dbus: decl.defaults.dbus.clone(),
            dbus_args: String::new(),
            shm: decl.defaults.shm.clone(),
            tmp: decl.defaults.tmp.clone(),
            hw: decl.defaults.hw,
            sec: decl.defaults.sec,
            runtime: decl.defaults.runtime,
            seccomp_allowed: decl.default_allowed.clone(),
            bridges: Vec::new(),
            bwrap_args: String::new(),
            extra_env: String::new(),
            extra_paths: String::new(),
        }
    }

    #[must_use]
    pub fn umu_defaults(decl: &SbRunConfig) -> Self {
        let umu_allows = [
            "unshare",
            "setns",
            "mount",
            "umount",
            "umount2",
            "pivot_root",
            "chroot",
            "open_tree",
            "move_mount",
            "fsopen",
            "fsconfig",
            "fsmount",
            "fspick",
            "mount_setattr",
            "clone_newuser",
            "ptrace",
            "process_vm_readv",
            "process_vm_writev",
            "modify_ldt",
            "personality",
            "userfaultfd",
        ];

        let mut data = Self::from_decl(decl);
        data.x11 = "passthrough".into();
        data.hw.gpu = true;
        data.sec.landlock = false;
        data.runtime.tmpfs = false;
        for s in umu_allows {
            if !data.seccomp_allowed.iter().any(|x| x == s) {
                data.seccomp_allowed.push(s.to_string());
            }
        }
        data
    }

    #[must_use]
    pub fn seccomp_diff(&self, decl: &SbRunConfig) -> (Vec<String>, Vec<String>) {
        let def_set: HashSet<&str> = decl.default_allowed.iter().map(String::as_str).collect();
        let cur_set: HashSet<&str> = self.seccomp_allowed.iter().map(String::as_str).collect();

        let allow: Vec<String> = self
            .seccomp_allowed
            .iter()
            .filter(|s| !def_set.contains(s.as_str()))
            .cloned()
            .collect();
        let block: Vec<String> = decl
            .default_allowed
            .iter()
            .filter(|s| !cur_set.contains(s.as_str()))
            .cloned()
            .collect();
        (allow, block)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SandboxUiMode {
    Standalone,
    Umu,
}

#[derive(Clone)]
pub struct BridgesListWidget {
    container: gtk4::Box,
    list_box: gtk4::ListBox,
    rules: Rc<RefCell<Vec<String>>>,
}

fn create_bridge_row(
    rule: &str,
    rules: &Rc<RefCell<Vec<String>>>,
    list_box: &gtk4::ListBox,
) -> gtk4::ListBoxRow {
    let mut parts = rule.splitn(3, ':');
    let dir = parts.next().unwrap_or("from");
    let addr = parts.next().unwrap_or("127.0.0.1");
    let port_list = parts.next().unwrap_or("");

    let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let dir_lbl = if dir == "from" {
        "[Из песочницы -> Хост]"
    } else {
        "[Хост -> Песочница]"
    };
    let label = gtk4::Label::builder()
        .label(format!("{dir_lbl} {addr}:[{port_list}]"))
        .xalign(0.0)
        .hexpand(true)
        .build();
    let btn_del = gtk4::Button::with_label("✕");

    row_box.append(&label);
    row_box.append(&btn_del);

    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&row_box));

    let rc_del = Rc::clone(rules);
    let lc_del = list_box.clone();
    let row_del = row.clone();
    btn_del.connect_clicked(move |_| {
        let idx = row_del.index();
        if let Ok(pos) = usize::try_from(idx)
            && pos < rc_del.borrow().len()
        {
            rc_del.borrow_mut().remove(pos);
        }
        lc_del.remove(&row_del);
    });

    row
}

impl BridgesListWidget {
    #[must_use]
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let rules = Rc::new(RefCell::new(Vec::new()));

        let title = gtk4::Label::builder()
            .label("Сетевые мосты TCP (rust-bridge):")
            .xalign(0.0)
            .margin_top(4)
            .build();
        container.append(&title);

        let input_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

        let cmb_dir = gtk4::ComboBoxText::new();
        cmb_dir.append(Some("from"), "Из песочницы");
        cmb_dir.append(Some("to"), "В песочницу");
        cmb_dir.set_active_id(Some("from"));
        input_row.append(&cmb_dir);

        let ent_addr = gtk4::Entry::new();
        ent_addr.set_text("127.0.0.1");
        ent_addr.set_width_chars(11);
        input_row.append(&ent_addr);

        let ent_ports = gtk4::Entry::new();
        ent_ports.set_placeholder_text(Some("57343,27060"));
        ent_ports.set_hexpand(true);
        input_row.append(&ent_ports);

        let btn_add = gtk4::Button::with_label("+ Добавить");
        input_row.append(&btn_add);

        let btn_steam = gtk4::Button::with_label("+ Steam");
        input_row.append(&btn_steam);

        container.append(&input_row);

        let list_box = gtk4::ListBox::new();
        list_box.add_css_class("boxed-list");
        container.append(&list_box);

        let w = Self {
            container,
            list_box,
            rules,
        };

        let w_add = w.clone();
        btn_add.connect_clicked(move |_| {
            let dir = cmb_dir.active_id().unwrap_or_else(|| "from".into());
            let addr = ent_addr.text().to_string();
            let ports = ent_ports.text().to_string();
            if !ports.trim().is_empty() {
                w_add.add_rule(&format!("{dir}:{addr}:{ports}"));
                ent_ports.set_text("");
            }
        });

        let w_steam = w.clone();
        btn_steam.connect_clicked(move |_| {
            w_steam.add_rule("from:127.0.0.1:57343,27060");
        });

        w
    }

    pub fn add_rule(&self, rule_str: &str) {
        let trimmed = rule_str.trim();
        if trimmed.is_empty() {
            return;
        }
        self.rules.borrow_mut().push(trimmed.to_string());
        let row = create_bridge_row(trimmed, &self.rules, &self.list_box);
        self.list_box.append(&row);
    }

    pub fn load_rules(&self, items: &[String]) {
        self.rules.borrow_mut().clear();
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
        for item in items {
            self.add_rule(item);
        }
    }

    #[must_use]
    pub const fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    #[must_use]
    pub fn get_rules(&self) -> Vec<String> {
        self.rules.borrow().clone()
    }
}

impl Default for BridgesListWidget {
    fn default() -> Self {
        Self::new()
    }
}

fn show_error_dialog(parent: &gtk4::Window, title: &str, body: &str) {
    let dlg = gtk4::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk4::MessageType::Error)
        .buttons(gtk4::ButtonsType::Ok)
        .text(title)
        .secondary_text(body)
        .build();
    dlg.connect_response(|d, _| d.destroy());
    dlg.present();
}

fn create_temp_edit_file(prefix: &str, content: &str) -> Result<PathBuf, String> {
    let runtime = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let seq = EDIT_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = PathBuf::from(runtime).join(format!("{prefix}-{pid}-{seq}.json"));
    fs::write(&path, content).map_err(|e| format!("failed to write '{}': {e}", path.display()))?;
    Ok(path)
}

fn spawn_neovide_json_editor<F>(
    parent: &gtk4::Window,
    neovide_bin: &str,
    prefix: &str,
    initial_json: &str,
    on_saved: F,
) where
    F: Fn(String) + 'static,
{
    let tmp_path = match create_temp_edit_file(prefix, initial_json) {
        Ok(p) => p,
        Err(e) => {
            show_error_dialog(parent, "Ошибка создания временного файла", &e);
            return;
        }
    };

    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    let bin = neovide_bin.to_string();
    let path_thread = tmp_path;

    thread::spawn(move || {
        let status = Command::new(&bin).arg(&path_thread).status();
        let res = match status {
            Ok(s) if s.success() => fs::read_to_string(&path_thread)
                .map_err(|e| format!("Не удалось прочитать файл: {e}")),
            Ok(s) => Err(format!("Neovide завершился с кодом {:?}", s.code())),
            Err(e) => Err(format!("Не удалось запустить '{bin}': {e}")),
        };
        let _ = fs::remove_file(&path_thread);
        let _ = tx.send(res);
    });

    let p_win = parent.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(120), move || {
        match rx.try_recv() {
            Ok(Ok(edited)) => {
                on_saved(edited);
                glib::ControlFlow::Break
            }
            Ok(Err(err)) => {
                show_error_dialog(&p_win, "Ошибка редактора Neovide", &err);
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}

fn read_singbox_source_pretty(decl: &SbRunConfig, current: &str) -> Result<String, String> {
    let trimmed = current.trim();
    let raw = if trimmed.is_empty() {
        fs::read_to_string(&decl.singbox_config)
            .map_err(|e| format!("Не удалось прочитать '{}': {e}", decl.singbox_config))?
    } else if trimmed.starts_with('{') {
        trimmed.to_string()
    } else {
        fs::read_to_string(trimmed).map_err(|e| format!("Не удалось прочитать '{trimmed}': {e}"))?
    };

    let val: Value =
        serde_json::from_str(&raw).map_err(|e| format!("Некорректный JSON sing-box: {e}"))?;
    serde_json::to_string_pretty(&val).map_err(|e| format!("Ошибка сериализации JSON: {e}"))
}

fn open_singbox_neovide_editor(parent: &gtk4::Window, decl: &SbRunConfig, entry: &gtk4::Entry) {
    let current = entry.text().to_string();
    let pretty = match read_singbox_source_pretty(decl, &current) {
        Ok(s) => s,
        Err(e) => {
            show_error_dialog(parent, "Ошибка конфигурации sing-box", &e);
            return;
        }
    };

    let default_compact = fs::read_to_string(&decl.singbox_config)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| serde_json::to_string(&v).ok())
        .unwrap_or_default();

    let p_win = parent.clone();
    let ent = entry.clone();
    spawn_neovide_json_editor(
        parent,
        &decl.neovide_bin,
        "sb-singbox-edit",
        &pretty,
        move |edited| match serde_json::from_str::<Value>(&edited) {
            Ok(val) => {
                if let Ok(compact) = serde_json::to_string(&val) {
                    if compact == default_compact {
                        ent.set_text("");
                    } else {
                        ent.set_text(&compact);
                    }
                }
            }
            Err(e) => {
                show_error_dialog(
                    &p_win,
                    "Некорректный JSON sing-box",
                    &format!("Изменения отклонены: {e}"),
                );
            }
        },
    );
}

fn build_syscall_list_rows(
    list_box: &gtk4::ListBox,
    blocked: &Rc<RefCell<Vec<String>>>,
    query: &str,
    btn_label: &gtk4::Button,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    let q = query.trim().to_lowercase();
    let current_len = blocked.borrow().len();
    btn_label.set_label(&format!("Syscalls ({current_len})..."));

    let items = blocked.borrow().clone();
    for name in items {
        if !q.is_empty() && !name.to_lowercase().contains(&q) {
            continue;
        }
        let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let lbl = gtk4::Label::builder()
            .label(&name)
            .xalign(0.0)
            .hexpand(true)
            .build();
        let btn_del = gtk4::Button::with_label("✕");
        row_box.append(&lbl);
        row_box.append(&btn_del);

        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&row_box));
        list_box.append(&row);

        let b_rc = Rc::clone(blocked);
        let lb_c = list_box.clone();
        let btn_c = btn_label.clone();
        let name_del = name.clone();
        let q_del = q.clone();
        btn_del.connect_clicked(move |_| {
            b_rc.borrow_mut().retain(|s| s != &name_del);
            build_syscall_list_rows(&lb_c, &b_rc, &q_del, &btn_c);
        });
    }
}

fn build_seccomp_add_row(
    decl: &SbRunConfig,
    blocked: &Rc<RefCell<Vec<String>>>,
    list_box: &gtk4::ListBox,
    search: &gtk4::SearchEntry,
    btn_summary: &gtk4::Button,
    lbl_err: &gtk4::Label,
) -> gtk4::Box {
    let add_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let ent_sys = gtk4::Entry::builder()
        .placeholder_text("Имя syscall (напр. ptrace, unshare, bpf)...")
        .hexpand(true)
        .build();

    let store = gtk4::ListStore::new(&[glib::Type::STRING]);
    for name in &decl.syscalls {
        store.insert_with_values(None, &[(0, name)]);
    }
    let completion = gtk4::EntryCompletion::new();
    completion.set_model(Some(&store));
    completion.set_text_column(0);
    completion.set_minimum_key_length(1);
    #[allow(deprecated)]
    ent_sys.set_completion(Some(&completion));

    let btn_add = gtk4::Button::with_label("+ Разрешить");
    add_box.append(&ent_sys);
    add_box.append(&btn_add);

    let b_add = Rc::clone(blocked);
    let lb_add = list_box.clone();
    let summary_add = btn_summary.clone();
    let s_add = search.clone();
    let decl_add = decl.clone();
    let err_lbl = lbl_err.clone();
    btn_add.connect_clicked(move |_| {
        let name = ent_sys.text().trim().to_string();
        if name.is_empty() {
            return;
        }
        if !decl_add.is_valid_syscall(&name) {
            err_lbl.set_markup(&format!(
                "<span foreground='red'>Неизвестный syscall: {name}</span>"
            ));
            return;
        }
        let mut list = b_add.borrow_mut();
        if !list.contains(&name) {
            list.push(name);
        }
        drop(list);
        err_lbl.set_text("");
        ent_sys.set_text("");
        build_syscall_list_rows(&lb_add, &b_add, &s_add.text(), &summary_add);
    });

    add_box
}

fn open_seccomp_editor_dialog(
    parent: &gtk4::Window,
    decl: &SbRunConfig,
    blocked: &Rc<RefCell<Vec<String>>>,
    btn_summary: &gtk4::Button,
    default_preset: &[String],
) {
    let dlg = gtk4::Window::builder()
        .title("Настройка Seccomp (разрешенные syscalls)")
        .default_width(520)
        .default_height(520)
        .transient_for(parent)
        .modal(true)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    dlg.set_child(Some(&vbox));

    let search = gtk4::SearchEntry::builder()
        .placeholder_text("Поиск среди разрешенных...")
        .build();
    vbox.append(&search);

    let lbl_status = gtk4::Label::builder().xalign(0.0).build();
    let list_box = gtk4::ListBox::new();
    list_box.set_selection_mode(gtk4::SelectionMode::None);
    list_box.add_css_class("boxed-list");

    vbox.append(&build_seccomp_add_row(
        decl,
        blocked,
        &list_box,
        &search,
        btn_summary,
        &lbl_status,
    ));
    vbox.append(&lbl_status);

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list_box)
        .build();
    vbox.append(&scroll);

    build_syscall_list_rows(&list_box, blocked, "", btn_summary);

    let lb_search = list_box.clone();
    let b_search = Rc::clone(blocked);
    let summary_search = btn_summary.clone();
    search.connect_search_changed(move |s| {
        build_syscall_list_rows(&lb_search, &b_search, &s.text(), &summary_search);
    });

    let bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    bbox.set_halign(gtk4::Align::End);
    let btn_nvim = gtk4::Button::with_label("Neovide (JSON)");
    let btn_reset = gtk4::Button::with_label("По умолчанию");
    let btn_done = gtk4::Button::with_label("Готово");
    btn_done.add_css_class("suggested-action");
    bbox.append(&btn_nvim);
    bbox.append(&btn_reset);
    bbox.append(&btn_done);
    vbox.append(&bbox);

    connect_seccomp_neovide_btn(
        &btn_nvim,
        &dlg,
        decl,
        blocked,
        &list_box,
        btn_summary,
        &search,
    );

    let b_rst = Rc::clone(blocked);
    let lb_rst = list_box;
    let summary_reset = btn_summary.clone();
    let preset_vec = default_preset.to_vec();
    btn_reset.connect_clicked(move |_| {
        (*b_rst.borrow_mut()).clone_from(&preset_vec);
        build_syscall_list_rows(&lb_rst, &b_rst, &search.text(), &summary_reset);
    });

    let dlg_c = dlg.clone();
    btn_done.connect_clicked(move |_| dlg_c.close());
    dlg.present();
}

fn connect_seccomp_neovide_btn(
    btn_nvim: &gtk4::Button,
    dlg: &gtk4::Window,
    decl: &SbRunConfig,
    blocked: &Rc<RefCell<Vec<String>>>,
    list_box: &gtk4::ListBox,
    btn_summary: &gtk4::Button,
    search: &gtk4::SearchEntry,
) {
    let b_nv = Rc::clone(blocked);
    let lb_nv = list_box.clone();
    let summary_nv = btn_summary.clone();
    let s_nv = search.clone();
    let dlg_nv = dlg.clone();
    let decl_nv = decl.clone();
    btn_nvim.connect_clicked(move |_| {
        let Ok(pretty) = serde_json::to_string_pretty(&*b_nv.borrow()) else {
            return;
        };
        let b_cb = Rc::clone(&b_nv);
        let lb_cb = lb_nv.clone();
        let btn_cb = summary_nv.clone();
        let s_cb = s_nv.clone();
        let d_cb = dlg_nv.clone();
        let cfg_cb = decl_nv.clone();
        spawn_neovide_json_editor(
            &dlg_nv,
            &decl_nv.neovide_bin,
            "sb-seccomp-edit",
            &pretty,
            move |edited| match serde_json::from_str::<Vec<String>>(&edited) {
                Ok(new_list) => {
                    for name in &new_list {
                        if !cfg_cb.is_valid_syscall(name) {
                            show_error_dialog(
                                &d_cb,
                                "Неизвестный syscall",
                                &format!("Syscall '{name}' не найден в таблице ядра."),
                            );
                            return;
                        }
                    }
                    *b_cb.borrow_mut() = new_list;
                    build_syscall_list_rows(&lb_cb, &b_cb, &s_cb.text(), &btn_cb);
                }
                Err(e) => show_error_dialog(&d_cb, "Некорректный JSON", &e.to_string()),
            },
        );
    });
}

#[must_use]
pub fn create_labelled_combo(
    label_text: &str,
    options: &[(&str, &str)],
    default_id: &str,
) -> (gtk4::Box, gtk4::ComboBoxText) {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let lbl = gtk4::Label::builder().label(label_text).xalign(0.0).build();
    hbox.append(&lbl);

    let cmb = gtk4::ComboBoxText::new();
    for (id, name) in options {
        cmb.append(Some(id), name);
    }
    if !cmb.set_active_id(Some(default_id)) {
        cmb.set_active(Some(0));
    }
    cmb.set_hexpand(true);
    hbox.append(&cmb);

    (hbox, cmb)
}

#[derive(Clone)]
pub struct SandboxOptionsWidget {
    container: gtk4::Box,
    decl: SbRunConfig,
    mode: SandboxUiMode,
    pub combo_net: gtk4::ComboBoxText,
    pub entry_singbox: gtk4::Entry,
    pub combo_wayland: Option<gtk4::ComboBoxText>,
    pub combo_x11: gtk4::ComboBoxText,
    pub combo_pulse: gtk4::ComboBoxText,
    pub combo_pipewire: gtk4::ComboBoxText,
    pub toggle_gpu: gtk4::CheckButton,
    pub toggle_gamepad: gtk4::CheckButton,
    pub combo_webcam: gtk4::ComboBoxText,
    pub paths_widget: PathsListWidget,
    pub combo_dbus: gtk4::ComboBoxText,
    pub toggle_portals: gtk4::CheckButton,
    pub toggle_landlock: gtk4::CheckButton,
    pub toggle_seccomp: gtk4::CheckButton,
    pub btn_seccomp_edit: gtk4::Button,
    pub seccomp_allowed: Rc<RefCell<Vec<String>>>,
    pub toggle_tmpfs: gtk4::CheckButton,
    pub toggle_share_pid: gtk4::CheckButton,
    pub toggle_vpnify: gtk4::CheckButton,
    pub combo_shm: gtk4::ComboBoxText,
    pub combo_tmp: gtk4::ComboBoxText,
    pub bridges_widget: BridgesListWidget,
    pub entry_dbus: gtk4::Entry,
    pub entry_bwrap: gtk4::Entry,
    pub entry_env: gtk4::Entry,
}

struct SingboxRowControls {
    row: gtk4::Box,
    entry: gtk4::Entry,
}

fn build_singbox_row(
    parent: &gtk4::Window,
    decl: &SbRunConfig,
    initial_val: &str,
) -> SingboxRowControls {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let lbl = gtk4::Label::builder()
        .label("Конфиг Sing-Box:")
        .xalign(0.0)
        .build();
    let entry = gtk4::Entry::builder()
        .placeholder_text("По умолчанию, путь к JSON или строка {...}")
        .hexpand(true)
        .build();
    entry.set_text(initial_val);

    let btn_file = gtk4::Button::with_label("Файл...");
    let btn_nvim = gtk4::Button::with_label("Neovide");
    let btn_reset = gtk4::Button::with_label("✕");
    btn_reset.set_tooltip_text(Some("Сбросить к конфигу по умолчанию"));

    row.append(&lbl);
    row.append(&entry);
    row.append(&btn_file);
    row.append(&btn_nvim);
    row.append(&btn_reset);

    let w_file = parent.clone();
    let e_file = entry.clone();
    btn_file.connect_clicked(move |_| {
        let filter = gtk4::FileFilter::new();
        filter.set_name(Some("JSON (*.json)"));
        filter.add_pattern("*.json");
        let dlg = gtk4::FileChooserNative::builder()
            .title("Выберите JSON-конфиг sing-box")
            .transient_for(&w_file)
            .action(gtk4::FileChooserAction::Open)
            .build();
        dlg.add_filter(&filter);
        let ee = e_file.clone();
        dlg.connect_response(move |d, resp| {
            if resp == gtk4::ResponseType::Accept
                && let Some(f) = d.file()
                && let Some(p) = f.path()
            {
                ee.set_text(&p.to_string_lossy());
            }
            d.destroy();
        });
        dlg.show();
    });

    let w_nv = parent.clone();
    let d_nv = decl.clone();
    let e_nv = entry.clone();
    btn_nvim.connect_clicked(move |_| {
        open_singbox_neovide_editor(&w_nv, &d_nv, &e_nv);
    });

    let e_rst = entry.clone();
    btn_reset.connect_clicked(move |_| {
        e_rst.set_text("");
    });

    SingboxRowControls { row, entry }
}

struct SubsysGridWidgets {
    grid: gtk4::Grid,
    combo_wayland: Option<gtk4::ComboBoxText>,
    combo_x11: gtk4::ComboBoxText,
    combo_pulse: gtk4::ComboBoxText,
    combo_pipewire: gtk4::ComboBoxText,
    toggle_gpu: gtk4::CheckButton,
    toggle_gamepad: gtk4::CheckButton,
    combo_webcam: gtk4::ComboBoxText,
}

fn attach_display_and_audio_combos(
    grid: &gtk4::Grid,
    mode: SandboxUiMode,
    init: &SandboxFormData,
) -> (
    Option<gtk4::ComboBoxText>,
    gtk4::ComboBoxText,
    gtk4::ComboBoxText,
    gtk4::ComboBoxText,
    i32,
) {
    let mut row = 0;
    let (cmb_wl, cmb_x11) = if mode == SandboxUiMode::Standalone {
        let (wl_box, wl) = create_labelled_combo(
            "Wayland:",
            &[
                ("sandboxed", "Изолированный (way-secure)"),
                ("passthrough", "Прямой (хост)"),
                ("off", "Отключен"),
            ],
            &init.wayland,
        );
        let (x11_box, x11) = create_labelled_combo(
            "X11:",
            &[
                ("off", "Отключен"),
                ("passthrough", "Прямой (/tmp/.X11-unix)"),
                ("sandboxed", "Изолированный (xwayland-satellite)"),
            ],
            &init.x11,
        );
        grid.attach(&wl_box, 0, row, 1, 1);
        grid.attach(&x11_box, 1, row, 1, 1);
        row += 1;
        (Some(wl), x11)
    } else {
        let (x11_box, x11) = create_labelled_combo(
            "XWayland (X11):",
            &[
                ("passthrough", "Прямой доступ (Хост X11)"),
                ("sandboxed", "Изолированный (xwayland-satellite)"),
                ("off", "Отключен"),
            ],
            &init.x11,
        );
        grid.attach(&x11_box, 0, row, 2, 1);
        row += 1;
        (None, x11)
    };

    let (pw_box, cmb_pw) = create_labelled_combo(
        "PipeWire:",
        &[
            ("sandboxed", "Изолированный"),
            ("passthrough", "Прямой (хост)"),
            ("off", "Отключен"),
        ],
        &init.pipewire,
    );
    let (pulse_box, cmb_pulse) = create_labelled_combo(
        "PulseAudio:",
        &[
            ("sandboxed", "Изолированный"),
            ("passthrough", "Прямой (хост)"),
            ("off", "Отключен"),
        ],
        &init.pulse,
    );
    grid.attach(&pw_box, 0, row, 1, 1);
    grid.attach(&pulse_box, 1, row, 1, 1);
    row += 1;
    (cmb_wl, cmb_x11, cmb_pw, cmb_pulse, row)
}

fn build_subsys_grid(mode: SandboxUiMode, init: &SandboxFormData) -> SubsysGridWidgets {
    let grid = gtk4::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(8);

    let combo_wayland = attach_display_and_audio_combos(&grid, mode, init);
    let mut row = combo_wayland.4;

    let chk_gpu = gtk4::CheckButton::with_label("GPU / 3D Ускорение");
    chk_gpu.set_active(init.hw.gpu);
    let chk_gamepad = gtk4::CheckButton::with_label("Геймпад (/dev/input)");
    chk_gamepad.set_active(init.hw.gamepad);
    grid.attach(&chk_gpu, 0, row, 1, 1);
    grid.attach(&chk_gamepad, 1, row, 1, 1);
    row += 1;

    let (cam_box, cmb_cam) = create_labelled_combo(
        "Веб-камера:",
        &[
            ("0", "Отключена"),
            ("1", "1 устройство (/dev/video0)"),
            ("2", "2 устройства (/dev/video0..1)"),
            ("4", "4 устройства (/dev/video0..3)"),
        ],
        &init.webcam,
    );
    grid.attach(&cam_box, 0, row, 2, 1);

    SubsysGridWidgets {
        grid,
        combo_wayland: combo_wayland.0,
        combo_x11: combo_wayland.1,
        combo_pipewire: combo_wayland.2,
        combo_pulse: combo_wayland.3,
        toggle_gpu: chk_gpu,
        toggle_gamepad: chk_gamepad,
        combo_webcam: cmb_cam,
    }
}

struct AdvWidgets {
    expander: gtk4::Expander,
    combo_dbus: gtk4::ComboBoxText,
    toggle_portals: gtk4::CheckButton,
    toggle_landlock: gtk4::CheckButton,
    toggle_seccomp: gtk4::CheckButton,
    btn_seccomp_edit: gtk4::Button,
    seccomp_allowed: Rc<RefCell<Vec<String>>>,
    toggle_tmpfs: gtk4::CheckButton,
    toggle_share_pid: gtk4::CheckButton,
    toggle_vpnify: gtk4::CheckButton,
    combo_shm: gtk4::ComboBoxText,
    combo_tmp: gtk4::ComboBoxText,
    bridges_widget: BridgesListWidget,
    entry_dbus: gtk4::Entry,
    entry_bwrap: gtk4::Entry,
    entry_env: gtk4::Entry,
}

fn append_labeled_entry(
    parent: &gtk4::Box,
    label: &str,
    placeholder: &str,
    initial: &str,
) -> gtk4::Entry {
    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some(placeholder));
    entry.set_text(initial);
    parent.append(
        &gtk4::Label::builder()
            .label(label)
            .xalign(0.0)
            .margin_top(4)
            .build(),
    );
    parent.append(&entry);
    entry
}

fn build_seccomp_controls(
    parent: &gtk4::Window,
    decl: &SbRunConfig,
    mode: SandboxUiMode,
    init: &SandboxFormData,
) -> (
    gtk4::Box,
    gtk4::CheckButton,
    gtk4::Button,
    Rc<RefCell<Vec<String>>>,
) {
    let seccomp_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let chk_seccomp = gtk4::CheckButton::with_label("Seccomp фильтр");
    chk_seccomp.set_active(init.sec.seccomp);
    let seccomp_allowed = Rc::new(RefCell::new(init.seccomp_allowed.clone()));
    let btn_seccomp_edit =
        gtk4::Button::with_label(&format!("Syscalls ({})...", init.seccomp_allowed.len()));
    btn_seccomp_edit.set_sensitive(init.sec.seccomp);
    seccomp_box.append(&chk_seccomp);
    seccomp_box.append(&btn_seccomp_edit);

    let b_btn = btn_seccomp_edit.clone();
    chk_seccomp.connect_toggled(move |c| b_btn.set_sensitive(c.is_active()));

    let preset_defaults = if mode == SandboxUiMode::Umu {
        SandboxFormData::umu_defaults(decl).seccomp_allowed
    } else {
        decl.default_allowed.clone()
    };
    let w_sec = parent.clone();
    let d_sec = decl.clone();
    let blk_sec = Rc::clone(&seccomp_allowed);
    let b_sec_lbl = btn_seccomp_edit.clone();
    btn_seccomp_edit.connect_clicked(move |_| {
        open_seccomp_editor_dialog(&w_sec, &d_sec, &blk_sec, &b_sec_lbl, &preset_defaults);
    });

    (seccomp_box, chk_seccomp, btn_seccomp_edit, seccomp_allowed)
}

struct AdvGridControls {
    grid: gtk4::Grid,
    combo_dbus: gtk4::ComboBoxText,
    toggle_portals: gtk4::CheckButton,
    toggle_landlock: gtk4::CheckButton,
    toggle_tmpfs: gtk4::CheckButton,
    toggle_share_pid: gtk4::CheckButton,
    toggle_vpnify: gtk4::CheckButton,
    combo_shm: gtk4::ComboBoxText,
    combo_tmp: gtk4::ComboBoxText,
}

fn build_adv_toggles_grid(mode: SandboxUiMode, init: &SandboxFormData) -> AdvGridControls {
    let grid = gtk4::Grid::new();
    grid.set_column_spacing(14);
    grid.set_row_spacing(8);

    let (dbus_box, combo_dbus) = create_labelled_combo(
        "D-Bus:",
        &[
            ("sandboxed", "Изолированный (Прокси)"),
            ("passthrough", "Прямой (хост)"),
            ("off", "Отключен"),
        ],
        &init.dbus,
    );
    grid.attach(&dbus_box, 0, 0, 1, 1);

    let toggle_portals = gtk4::CheckButton::with_label("Порталы XDG (MIME)");
    toggle_portals.set_active(init.sec.portals);
    grid.attach(&toggle_portals, 1, 0, 1, 1);

    let toggle_landlock = gtk4::CheckButton::with_label("Landlock (сигналы)");
    toggle_landlock.set_active(init.sec.landlock);
    grid.attach(&toggle_landlock, 0, 1, 1, 1);

    let tmpfs_label = if mode == SandboxUiMode::Umu {
        "Одноразовый дом (Tmpfs)"
    } else {
        "Одноразовая среда (Tmpfs)"
    };
    let toggle_tmpfs = gtk4::CheckButton::with_label(tmpfs_label);
    toggle_tmpfs.set_active(init.runtime.tmpfs);
    grid.attach(&toggle_tmpfs, 1, 1, 1, 1);

    let toggle_share_pid = gtk4::CheckButton::with_label("Разделять PID (Share PID)");
    toggle_share_pid.set_active(init.runtime.share_pid);
    grid.attach(&toggle_share_pid, 0, 2, 1, 1);

    let toggle_vpnify = gtk4::CheckButton::with_label("VPN (vpnify)");
    toggle_vpnify.set_active(init.runtime.vpnify);
    grid.attach(&toggle_vpnify, 1, 2, 1, 1);

    let (shm_box, combo_shm) = create_labelled_combo(
        "/dev/shm:",
        &[
            ("sandboxed", "Изолированный"),
            ("passthrough", "Прямой (хост)"),
        ],
        &init.shm,
    );
    grid.attach(&shm_box, 0, 3, 1, 1);

    let (tmp_box, combo_tmp) = create_labelled_combo(
        "/tmp:",
        &[
            ("sandboxed", "Изолированный"),
            ("passthrough", "Прямой (хост)"),
        ],
        &init.tmp,
    );
    grid.attach(&tmp_box, 1, 3, 1, 1);

    AdvGridControls {
        grid,
        combo_dbus,
        toggle_portals,
        toggle_landlock,
        toggle_tmpfs,
        toggle_share_pid,
        toggle_vpnify,
        combo_shm,
        combo_tmp,
    }
}

fn build_adv_section(
    parent: &gtk4::Window,
    decl: &SbRunConfig,
    mode: SandboxUiMode,
    init: &SandboxFormData,
) -> AdvWidgets {
    let adv_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    adv_box.set_margin_top(6);
    adv_box.set_margin_bottom(6);
    adv_box.set_margin_start(4);
    adv_box.set_margin_end(4);

    let g = build_adv_toggles_grid(mode, init);
    let adv_grid = g.grid;

    let (seccomp_box, chk_seccomp, btn_seccomp_edit, seccomp_allowed) =
        build_seccomp_controls(parent, decl, mode, init);
    adv_grid.attach(&seccomp_box, 0, 4, 2, 1);

    adv_box.append(&adv_grid);
    let bridges_widget = BridgesListWidget::new();
    bridges_widget.load_rules(&init.bridges);
    adv_box.append(bridges_widget.widget());

    let dbus_entry = append_labeled_entry(
        &adv_box,
        "Дополнительные аргументы D-Bus:",
        "--dbus-talk=org.freedesktop.Notifications --dbus-own=org.example.App",
        &init.dbus_args,
    );
    let bwrap_entry = append_labeled_entry(
        &adv_box,
        "Дополнительные аргументы Bubblewrap:",
        "Например: --cap-add CAP_SYS_PTRACE",
        &init.bwrap_args,
    );
    let env_entry = append_labeled_entry(
        &adv_box,
        "Переменные среды (--env):",
        "KEY=VAL KEY2=VAL2",
        &init.extra_env,
    );

    let expander = gtk4::Expander::builder()
        .label("Дополнительные настройки")
        .child(&adv_box)
        .build();

    AdvWidgets {
        expander,
        combo_dbus: g.combo_dbus,
        toggle_portals: g.toggle_portals,
        toggle_landlock: g.toggle_landlock,
        toggle_seccomp: chk_seccomp,
        btn_seccomp_edit,
        seccomp_allowed,
        toggle_tmpfs: g.toggle_tmpfs,
        toggle_share_pid: g.toggle_share_pid,
        toggle_vpnify: g.toggle_vpnify,
        combo_shm: g.combo_shm,
        combo_tmp: g.combo_tmp,
        bridges_widget,
        entry_dbus: dbus_entry,
        entry_bwrap: bwrap_entry,
        entry_env: env_entry,
    }
}

impl SandboxOptionsWidget {
    #[must_use]
    pub fn new(
        parent: &gtk4::Window,
        decl: &SbRunConfig,
        mode: SandboxUiMode,
        initial: Option<&SandboxFormData>,
    ) -> Self {
        let defaults = if mode == SandboxUiMode::Umu {
            SandboxFormData::umu_defaults(decl)
        } else {
            SandboxFormData::from_decl(decl)
        };
        let init = initial.unwrap_or(&defaults);

        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 10);

        let (net_box, cmb_net) = create_labelled_combo(
            "Сеть:",
            &[
                ("sandboxed", "Изолированная (Pasta)"),
                ("singbox", "VPN Туннель (Sing-Box)"),
                ("passthrough", "Прямая (Хост)"),
                ("off", "Отключена"),
            ],
            &init.network,
        );
        container.append(&net_box);

        let sb_controls = build_singbox_row(parent, decl, &init.singbox_config);
        sb_controls.row.set_visible(init.network == "singbox");
        container.append(&sb_controls.row);

        let sb_row_vis = sb_controls.row;
        cmb_net.connect_changed(move |c| {
            sb_row_vis.set_visible(c.active_id().as_deref() == Some("singbox"));
        });

        let sub = build_subsys_grid(mode, init);
        container.append(&sub.grid);

        let paths_widget = PathsListWidget::new(parent);
        if !init.extra_paths.trim().is_empty() {
            paths_widget.load_from_string_args(&init.extra_paths);
        }
        container.append(paths_widget.widget());

        let adv = build_adv_section(parent, decl, mode, init);
        container.append(&adv.expander);

        Self {
            container,
            decl: decl.clone(),
            mode,
            combo_net: cmb_net,
            entry_singbox: sb_controls.entry,
            combo_wayland: sub.combo_wayland,
            combo_x11: sub.combo_x11,
            combo_pulse: sub.combo_pulse,
            combo_pipewire: sub.combo_pipewire,
            toggle_gpu: sub.toggle_gpu,
            toggle_gamepad: sub.toggle_gamepad,
            combo_webcam: sub.combo_webcam,
            paths_widget,
            combo_dbus: adv.combo_dbus,
            toggle_portals: adv.toggle_portals,
            toggle_landlock: adv.toggle_landlock,
            toggle_seccomp: adv.toggle_seccomp,
            btn_seccomp_edit: adv.btn_seccomp_edit,
            seccomp_allowed: adv.seccomp_allowed,
            toggle_tmpfs: adv.toggle_tmpfs,
            toggle_share_pid: adv.toggle_share_pid,
            toggle_vpnify: adv.toggle_vpnify,
            combo_shm: adv.combo_shm,
            combo_tmp: adv.combo_tmp,
            bridges_widget: adv.bridges_widget,
            entry_dbus: adv.entry_dbus,
            entry_bwrap: adv.entry_bwrap,
            entry_env: adv.entry_env,
        }
    }

    #[must_use]
    pub const fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    #[must_use]
    pub const fn decl(&self) -> &SbRunConfig {
        &self.decl
    }

    #[must_use]
    pub fn get_data(&self) -> SandboxFormData {
        let combo_val = |c: &gtk4::ComboBoxText, fallback: &str| -> String {
            c.active_id()
                .map_or_else(|| fallback.to_string(), |s| s.to_string())
        };
        SandboxFormData {
            network: combo_val(&self.combo_net, &self.decl.defaults.network),
            singbox_config: self.entry_singbox.text().trim().to_string(),
            wayland: self.combo_wayland.as_ref().map_or_else(
                || self.decl.defaults.wayland.clone(),
                |c| combo_val(c, &self.decl.defaults.wayland),
            ),
            x11: combo_val(&self.combo_x11, &self.decl.defaults.x11),
            pulse: combo_val(&self.combo_pulse, &self.decl.defaults.pulse),
            pipewire: combo_val(&self.combo_pipewire, &self.decl.defaults.pipewire),
            webcam: combo_val(&self.combo_webcam, "0"),
            dbus: combo_val(&self.combo_dbus, &self.decl.defaults.dbus),
            dbus_args: self.entry_dbus.text().to_string(),
            shm: combo_val(&self.combo_shm, &self.decl.defaults.shm),
            tmp: combo_val(&self.combo_tmp, &self.decl.defaults.tmp),
            hw: SandboxHwFlags {
                gpu: self.toggle_gpu.is_active(),
                gamepad: self.toggle_gamepad.is_active(),
            },
            sec: SandboxSecurityFlags {
                landlock: self.toggle_landlock.is_active(),
                seccomp: self.toggle_seccomp.is_active(),
                portals: self.toggle_portals.is_active(),
            },
            runtime: SandboxRuntimeFlags {
                tmpfs: self.toggle_tmpfs.is_active(),
                share_pid: self.toggle_share_pid.is_active(),
                vpnify: self.toggle_vpnify.is_active(),
            },
            seccomp_allowed: self.seccomp_allowed.borrow().clone(),
            bridges: self.bridges_widget.get_rules(),
            bwrap_args: self.entry_bwrap.text().to_string(),
            extra_env: self.entry_env.text().to_string(),
            extra_paths: self.paths_widget.to_string_args(),
        }
    }

    pub fn load_data(&self, data: &SandboxFormData) {
        self.combo_net.set_active_id(Some(&data.network));
        self.entry_singbox.set_text(&data.singbox_config);
        if let Some(ref c) = self.combo_wayland {
            c.set_active_id(Some(&data.wayland));
        }
        self.combo_x11.set_active_id(Some(&data.x11));
        self.combo_pulse.set_active_id(Some(&data.pulse));
        self.combo_pipewire.set_active_id(Some(&data.pipewire));
        self.toggle_gpu.set_active(data.hw.gpu);
        self.toggle_gamepad.set_active(data.hw.gamepad);
        self.combo_webcam.set_active_id(Some(&data.webcam));
        self.combo_dbus.set_active_id(Some(&data.dbus));
        self.toggle_portals.set_active(data.sec.portals);
        self.toggle_landlock.set_active(data.sec.landlock);
        self.toggle_seccomp.set_active(data.sec.seccomp);
        (*self.seccomp_allowed.borrow_mut()).clone_from(&data.seccomp_allowed);
        self.btn_seccomp_edit
            .set_label(&format!("Syscalls ({})...", data.seccomp_allowed.len()));
        self.toggle_tmpfs.set_active(data.runtime.tmpfs);
        self.toggle_share_pid.set_active(data.runtime.share_pid);
        self.toggle_vpnify.set_active(data.runtime.vpnify);
        self.combo_shm.set_active_id(Some(&data.shm));
        self.combo_tmp.set_active_id(Some(&data.tmp));
        self.bridges_widget.load_rules(&data.bridges);
        self.entry_dbus.set_text(&data.dbus_args);
        self.entry_bwrap.set_text(&data.bwrap_args);
        self.entry_env.set_text(&data.extra_env);
        if !data.extra_paths.trim().is_empty() {
            self.paths_widget.load_from_string_args(&data.extra_paths);
        }
    }

    #[must_use]
    pub fn build_sb_run_args(&self) -> Vec<String> {
        let d = self.get_data();
        let mut args = vec![
            "--net".into(),
            d.network.clone(),
            "--pulse".into(),
            d.pulse.clone(),
            "--pipewire".into(),
            d.pipewire.clone(),
            "--x11".into(),
            d.x11.clone(),
            "--dbus".into(),
            d.dbus.clone(),
            "--shm".into(),
            d.shm.clone(),
            "--tmp".into(),
            d.tmp.clone(),
        ];
        if self.mode == SandboxUiMode::Standalone {
            args.push("--wayland".into());
            args.push(d.wayland.clone());
        }
        if !d.singbox_config.is_empty() {
            if d.singbox_config.starts_with('{') {
                args.push("--singbox-json".into());
            } else {
                args.push("--singbox-config".into());
            }
            args.push(d.singbox_config.clone());
        }
        args.push(if d.hw.gpu { "--gpu" } else { "--no-gpu" }.into());
        if d.hw.gamepad {
            args.push("--gamepad".into());
        }
        if d.webcam != "0" {
            args.push("--webcam".into());
            args.push(d.webcam.clone());
        }
        args.push(
            if d.sec.landlock {
                "--landlock"
            } else {
                "--no-landlock"
            }
            .into(),
        );
        if !d.sec.portals {
            args.push("--no-portals".into());
        }
        if d.runtime.tmpfs {
            args.push("--tmpfs".into());
        }
        if d.runtime.share_pid {
            args.push("--share-pid".into());
        }
        if d.runtime.vpnify {
            args.push("--vpnify".into());
        }
        if d.sec.seccomp {
            args.push("--seccomp".into());
            let (allow, block) = d.seccomp_diff(&self.decl);
            if !allow.is_empty() {
                args.push("--seccomp-allow".into());
                args.push(allow.join(","));
            }
            if !block.is_empty() {
                args.push("--seccomp-block".into());
                args.push(block.join(","));
            }
        } else {
            args.push("--no-seccomp".into());
        }
        for b in &d.bridges {
            args.push("--bridge".into());
            args.push(b.clone());
        }
        for piece in d.bwrap_args.split_whitespace() {
            args.push("--bwrap-arg".into());
            args.push(piece.to_string());
        }
        for piece in d.dbus_args.split_whitespace() {
            args.push("--dbus-arg".into());
            args.push(piece.to_string());
        }
        for piece in d.extra_env.split_whitespace() {
            args.push("--env".into());
            args.push(piece.to_string());
        }
        args.extend(self.paths_widget.get_flags());
        args
    }

    pub fn add_path(&self, path: &str, mode: BindMode) {
        self.paths_widget.add_path(path, mode);
    }
}

#[must_use]
pub fn format_seccomp_summary(allowed: &[String], decl: &SbRunConfig) -> String {
    let mut s = String::new();
    let def_set: HashSet<&str> = decl.default_allowed.iter().map(String::as_str).collect();
    let cur_set: HashSet<&str> = allowed.iter().map(String::as_str).collect();
    let allow: Vec<&str> = allowed
        .iter()
        .filter(|x| !def_set.contains(x.as_str()))
        .map(String::as_str)
        .collect();
    let block: Vec<&str> = decl
        .default_allowed
        .iter()
        .filter(|x| !cur_set.contains(x.as_str()))
        .map(String::as_str)
        .collect();
    let _ = write!(s, "allow={} block={}", allow.join(","), block.join(","));
    s
}
