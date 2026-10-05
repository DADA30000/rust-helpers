use gtk4::prelude::*;
use std::cell::RefCell;
use std::env;
use std::process::Command;
use std::rc::Rc;
use system_ui_helpers::{PathsListWidget, apply_clean_theme};

#[derive(Clone)]
struct BridgesListWidget {
    container: gtk4::Box,
    rules: Rc<RefCell<Vec<String>>>,
}

fn create_bridge_row(
    dir: &str,
    addr: &str,
    ports: &str,
    rules: &Rc<RefCell<Vec<String>>>,
    list_box: &gtk4::ListBox,
) -> gtk4::ListBoxRow {
    let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let dir_lbl = if dir == "from" {
        "[Из песочницы -> Хост]"
    } else {
        "[Хост -> Песочница]"
    };
    let label = gtk4::Label::builder()
        .label(format!("{dir_lbl} {addr}:[{ports}]"))
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
    fn new() -> Self {
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
        cmb_dir.append(Some("from"), "Из песочницы (to host)");
        cmb_dir.append(Some("to"), "В песочницу (to sandbox)");
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

        let rules_clone = Rc::clone(&rules);
        let list_clone = list_box;
        let add_rule = move |dir: String, addr: String, ports: String| {
            if ports.trim().is_empty() {
                return;
            }
            let rule_str = format!("{dir}:{addr}:{ports}");
            rules_clone.borrow_mut().push(rule_str);
            let row = create_bridge_row(&dir, &addr, &ports, &rules_clone, &list_clone);
            list_clone.append(&row);
        };

        let add_cb = add_rule.clone();
        let cd = cmb_dir;
        let ea = ent_addr;
        let ep = ent_ports;
        btn_add.connect_clicked(move |_| {
            let dir = cd.active_id().unwrap_or_else(|| "from".into());
            let addr = ea.text().to_string();
            let ports = ep.text().to_string();
            add_cb(dir.to_string(), addr, ports);
            ep.set_text("");
        });

        btn_steam.connect_clicked(move |_| {
            add_rule("from".into(), "127.0.0.1".into(), "57343,27060".into());
        });

        Self { container, rules }
    }

    const fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    fn get_flags(&self) -> Vec<String> {
        let mut flags = Vec::new();
        for rule in self.rules.borrow().iter() {
            flags.push("--bridge".into());
            flags.push(rule.clone());
        }
        flags
    }
}

struct GuiControls {
    entry_exe: gtk4::Entry,
    entry_id: gtk4::Entry,
    combo_net: gtk4::ComboBoxText,
    toggle_gpu: gtk4::CheckButton,
    combo_pipewire: gtk4::ComboBoxText,
    combo_pulse: gtk4::ComboBoxText,
    combo_wayland: gtk4::ComboBoxText,
    combo_x11: gtk4::ComboBoxText,
    combo_webcam: gtk4::ComboBoxText,
    toggle_gamepad: gtk4::CheckButton,
    combo_dbus: gtk4::ComboBoxText,
    toggle_landlock: gtk4::CheckButton,
    toggle_tmpfs: gtk4::CheckButton,
    toggle_share_pid: gtk4::CheckButton,
    toggle_portals: gtk4::CheckButton,
    toggle_vpnify: gtk4::CheckButton,
    combo_shm: gtk4::ComboBoxText,
    combo_tmp: gtk4::ComboBoxText,
    bridges_widget: BridgesListWidget,
    entry_dbus: gtk4::Entry,
    entry_bwrap: gtk4::Entry,
    paths_widget: PathsListWidget,
}

struct AdvControls {
    combo_dbus: gtk4::ComboBoxText,
    toggle_landlock: gtk4::CheckButton,
    toggle_tmpfs: gtk4::CheckButton,
    toggle_share_pid: gtk4::CheckButton,
    toggle_portals: gtk4::CheckButton,
    toggle_vpnify: gtk4::CheckButton,
    combo_shm: gtk4::ComboBoxText,
    combo_tmp: gtk4::ComboBoxText,
    bridges_widget: BridgesListWidget,
    entry_dbus: gtk4::Entry,
    entry_bwrap: gtk4::Entry,
}

fn create_labelled_combo(
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
    cmb.set_active_id(Some(default_id));
    cmb.set_hexpand(true);
    hbox.append(&cmb);

    (hbox, cmb)
}

fn build_adv_expander() -> (gtk4::Expander, AdvControls) {
    let adv_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    adv_box.set_margin_top(6);
    adv_box.set_margin_bottom(6);
    adv_box.set_margin_start(4);
    adv_box.set_margin_end(4);

    let adv_grid = gtk4::Grid::new();
    adv_grid.set_column_spacing(16);
    adv_grid.set_row_spacing(8);

    let (dbus_box, cmb_dbus) = create_labelled_combo(
        "D-Bus:",
        &[
            ("sandboxed", "Изолированный (Прокси)"),
            ("passthrough", "Прямой (хост)"),
            ("off", "Отключен"),
        ],
        "sandboxed",
    );
    adv_grid.attach(&dbus_box, 0, 0, 1, 1);

    let chk_portals = gtk4::CheckButton::with_label("Порталы XDG (MIME)");
    chk_portals.set_active(true);
    adv_grid.attach(&chk_portals, 1, 0, 1, 1);

    let chk_landlock = gtk4::CheckButton::with_label("Landlock (сигналы)");
    chk_landlock.set_active(true);
    adv_grid.attach(&chk_landlock, 0, 1, 1, 1);

    let chk_tmpfs = gtk4::CheckButton::with_label("Одноразовая среда (Tmpfs)");
    chk_tmpfs.set_active(true);
    adv_grid.attach(&chk_tmpfs, 1, 1, 1, 1);

    let chk_share_pid = gtk4::CheckButton::with_label("Разделять PID (Share PID)");
    chk_share_pid.set_active(false);
    adv_grid.attach(&chk_share_pid, 0, 2, 1, 1);

    let chk_vpnify = gtk4::CheckButton::with_label("VPN (vpnify)");
    chk_vpnify.set_active(false);
    adv_grid.attach(&chk_vpnify, 1, 2, 1, 1);

    let (shm_box, cmb_shm) = create_labelled_combo(
        "/dev/shm:",
        &[
            ("sandboxed", "Изолированный"),
            ("passthrough", "Прямой (хост)"),
        ],
        "sandboxed",
    );
    adv_grid.attach(&shm_box, 0, 3, 1, 1);

    let (tmp_box, cmb_tmp) = create_labelled_combo(
        "/tmp:",
        &[
            ("sandboxed", "Изолированный"),
            ("passthrough", "Прямой (хост)"),
        ],
        "sandboxed",
    );
    adv_grid.attach(&tmp_box, 1, 3, 1, 1);

    adv_box.append(&adv_grid);

    let bridges_widget = BridgesListWidget::new();
    adv_box.append(bridges_widget.widget());

    let dbus_label = gtk4::Label::builder()
        .label("Дополнительные аргументы D-Bus:")
        .xalign(0.0)
        .margin_top(4)
        .build();
    adv_box.append(&dbus_label);

    let dbus_entry = gtk4::Entry::new();
    dbus_entry.set_placeholder_text(Some(
        "Например: --talk=org.freedesktop.Notifications --own=org.example.App",
    ));
    dbus_entry.set_hexpand(true);
    adv_box.append(&dbus_entry);

    let bwrap_label = gtk4::Label::builder()
        .label("Дополнительные аргументы Bubblewrap:")
        .xalign(0.0)
        .margin_top(4)
        .build();
    adv_box.append(&bwrap_label);

    let bwrap_entry = gtk4::Entry::new();
    bwrap_entry.set_placeholder_text(Some("Например: --cap-add CAP_SYS_PTRACE"));
    bwrap_entry.set_hexpand(true);
    adv_box.append(&bwrap_entry);

    let adv_expander = gtk4::Expander::builder()
        .label("Дополнительные настройки")
        .child(&adv_box)
        .build();

    (
        adv_expander,
        AdvControls {
            combo_dbus: cmb_dbus,
            toggle_landlock: chk_landlock,
            toggle_tmpfs: chk_tmpfs,
            toggle_share_pid: chk_share_pid,
            toggle_portals: chk_portals,
            toggle_vpnify: chk_vpnify,
            combo_shm: cmb_shm,
            combo_tmp: cmb_tmp,
            bridges_widget,
            entry_dbus: dbus_entry,
            entry_bwrap: bwrap_entry,
        },
    )
}

fn build_subsystem_grid() -> (
    gtk4::Grid,
    gtk4::CheckButton,
    gtk4::ComboBoxText,
    gtk4::ComboBoxText,
    gtk4::ComboBoxText,
    gtk4::ComboBoxText,
    gtk4::ComboBoxText,
    gtk4::CheckButton,
) {
    let grid = gtk4::Grid::new();
    grid.set_column_spacing(16);
    grid.set_row_spacing(8);

    let (wayland_box, cmb_wayland) = create_labelled_combo(
        "Wayland:",
        &[
            ("sandboxed", "Изолированный (way-secure)"),
            ("passthrough", "Прямой (хост)"),
            ("off", "Отключен"),
        ],
        "sandboxed",
    );
    grid.attach(&wayland_box, 0, 0, 1, 1);

    let (x11_box, cmb_x11) = create_labelled_combo(
        "X11:",
        &[
            ("off", "Отключен"),
            ("passthrough", "Прямой (/tmp/.X11-unix)"),
        ],
        "off",
    );
    grid.attach(&x11_box, 1, 0, 1, 1);

    let (pw_box, cmb_pw) = create_labelled_combo(
        "PipeWire:",
        &[
            ("sandboxed", "Изолированный (restricted)"),
            ("passthrough", "Прямой (хост)"),
            ("off", "Отключен"),
        ],
        "sandboxed",
    );
    grid.attach(&pw_box, 0, 1, 1, 1);

    let (pulse_box, cmb_pulse) = create_labelled_combo(
        "PulseAudio:",
        &[
            ("sandboxed", "Изолированный (restricted)"),
            ("passthrough", "Прямой (хост)"),
            ("off", "Отключен"),
        ],
        "sandboxed",
    );
    grid.attach(&pulse_box, 1, 1, 1, 1);

    let chk_gpu = gtk4::CheckButton::with_label("GPU / 3D Ускорение");
    chk_gpu.set_active(true);
    grid.attach(&chk_gpu, 0, 2, 1, 1);

    let chk_gamepad = gtk4::CheckButton::with_label("Геймпад / Контроллер");
    chk_gamepad.set_active(false);
    grid.attach(&chk_gamepad, 1, 2, 1, 1);

    let (webcam_box, cmb_webcam) = create_labelled_combo(
        "Веб-камера:",
        &[
            ("0", "Отключена"),
            ("1", "1 устройство (/dev/video0)"),
            ("2", "2 устройства (/dev/video0..1)"),
            ("4", "4 устройства (/dev/video0..3)"),
        ],
        "0",
    );
    grid.attach(&webcam_box, 0, 3, 2, 1);

    (
        grid,
        chk_gpu,
        cmb_pw,
        cmb_pulse,
        cmb_wayland,
        cmb_x11,
        cmb_webcam,
        chk_gamepad,
    )
}

fn build_header_inputs(
    win: &gtk4::Window,
) -> (gtk4::Label, gtk4::Box, gtk4::Box, gtk4::Entry, gtk4::Entry) {
    let exe_label = gtk4::Label::builder()
        .label("Исполняемый файл:")
        .xalign(0.0)
        .build();

    let exe_entry = gtk4::Entry::new();
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        exe_entry.set_text(&args[1]);
    }
    exe_entry.set_hexpand(true);

    let btn_browse = gtk4::Button::with_label("Обзор...");
    let exe_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    exe_box.append(&exe_entry);
    exe_box.append(&btn_browse);

    let w_browse = win.clone();
    let ee_browse = exe_entry.clone();
    let id_entry = gtk4::Entry::new();
    id_entry.set_placeholder_text(Some("Авто (например: blender, app-firefox, test)"));
    id_entry.set_hexpand(true);

    let mut rng_bytes = [0u8; 4];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut rng_bytes);
    }
    let default_id = format!(
        "tmp-{:02x}{:02x}{:02x}{:02x}",
        rng_bytes[0], rng_bytes[1], rng_bytes[2], rng_bytes[3]
    );
    id_entry.set_text(&default_id);

    let id_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let id_label = gtk4::Label::builder()
        .label("Идентификатор (ID):")
        .xalign(0.0)
        .build();
    id_box.append(&id_label);
    id_box.append(&id_entry);

    let ie_change = id_entry.clone();
    exe_entry.connect_changed(move |e| {
        let text = e.text().to_string();
        if !text.is_empty() && ie_change.text().is_empty() {
            let p = std::path::Path::new(&text);
            if let Some(stem) = p.file_stem() {
                ie_change.set_text(&stem.to_string_lossy());
            }
        }
    });

    btn_browse.connect_clicked(move |_| {
        let dlg = gtk4::FileChooserNative::builder()
            .title("Выберите программу для запуска")
            .transient_for(&w_browse)
            .action(gtk4::FileChooserAction::Open)
            .build();
        let ee = ee_browse.clone();
        dlg.connect_response(move |d, response| {
            if response == gtk4::ResponseType::Accept
                && let Some(f) = d.file()
                && let Some(path) = f.path()
            {
                ee.set_text(&path.to_string_lossy());
            }
            d.destroy();
        });
        dlg.show();
    });

    (exe_label, exe_box, id_box, exe_entry, id_entry)
}

fn build_gui_window() -> (gtk4::Window, GuiControls) {
    let win = gtk4::Window::builder()
        .title("Запуск в изолированной песочнице")
        .default_width(620)
        .default_height(720)
        .resizable(false)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    vbox.set_margin_top(15);
    vbox.set_margin_bottom(15);
    vbox.set_margin_start(15);
    vbox.set_margin_end(15);

    let (exe_label, exe_box, id_box, exe_entry, id_entry) = build_header_inputs(&win);
    vbox.append(&exe_label);
    vbox.append(&exe_box);
    vbox.append(&id_box);

    let (net_box, cmb_net) = create_labelled_combo(
        "Сеть:",
        &[
            ("sandboxed", "Изолированная (Pasta)"),
            ("singbox", "VPN Туннель (Sing-Box)"),
            ("passthrough", "Прямая (Хост)"),
            ("off", "Отключена"),
        ],
        "sandboxed",
    );
    vbox.append(&net_box);

    let (grid, chk_gpu, cmb_pw, cmb_pulse, cmb_wayland, cmb_x11, cmb_webcam, chk_gamepad) =
        build_subsystem_grid();
    vbox.append(&grid);

    let paths_widget = PathsListWidget::new(&win);
    vbox.append(paths_widget.widget());

    let (adv_expander, adv_controls) = build_adv_expander();
    vbox.append(&adv_expander);

    let root_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    root_box.set_margin_top(12);
    root_box.set_margin_bottom(12);
    root_box.set_margin_start(12);
    root_box.set_margin_end(12);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&vbox)
        .build();
    root_box.append(&scrolled);
    win.set_child(Some(&root_box));

    (
        win,
        GuiControls {
            entry_exe: exe_entry,
            entry_id: id_entry,
            combo_net: cmb_net,
            toggle_gpu: chk_gpu,
            combo_pipewire: cmb_pw,
            combo_pulse: cmb_pulse,
            combo_wayland: cmb_wayland,
            combo_x11: cmb_x11,
            combo_webcam: cmb_webcam,
            toggle_gamepad: chk_gamepad,
            combo_dbus: adv_controls.combo_dbus,
            toggle_landlock: adv_controls.toggle_landlock,
            toggle_tmpfs: adv_controls.toggle_tmpfs,
            toggle_share_pid: adv_controls.toggle_share_pid,
            toggle_portals: adv_controls.toggle_portals,
            toggle_vpnify: adv_controls.toggle_vpnify,
            combo_shm: adv_controls.combo_shm,
            combo_tmp: adv_controls.combo_tmp,
            bridges_widget: adv_controls.bridges_widget,
            entry_dbus: adv_controls.entry_dbus,
            entry_bwrap: adv_controls.entry_bwrap,
            paths_widget,
        },
    )
}

fn append_subsystem_args(cmd: &mut Command, controls: &GuiControls) {
    if controls.toggle_gpu.is_active() {
        cmd.arg("--gpu");
    }
    let pw_id = controls
        .combo_pipewire
        .active_id()
        .unwrap_or_else(|| "sandboxed".into());
    cmd.arg("--pipewire").arg(pw_id.as_str());

    let pulse_id = controls
        .combo_pulse
        .active_id()
        .unwrap_or_else(|| "sandboxed".into());
    cmd.arg("--pulse").arg(pulse_id.as_str());

    let wayland_id = controls
        .combo_wayland
        .active_id()
        .unwrap_or_else(|| "sandboxed".into());
    cmd.arg("--wayland").arg(wayland_id.as_str());

    let x11_id = controls
        .combo_x11
        .active_id()
        .unwrap_or_else(|| "off".into());
    cmd.arg("--x11").arg(x11_id.as_str());

    if let Some(webcam_id) = controls.combo_webcam.active_id()
        && webcam_id != "0"
    {
        cmd.arg("--webcam").arg(webcam_id.as_str());
    }

    if controls.toggle_gamepad.is_active() {
        cmd.arg("--gamepad");
    }
}

fn append_adv_args(cmd: &mut Command, controls: &GuiControls) {
    let dbus_id = controls
        .combo_dbus
        .active_id()
        .unwrap_or_else(|| "sandboxed".into());
    cmd.arg("--dbus").arg(dbus_id.as_str());

    if !controls.toggle_landlock.is_active() {
        cmd.arg("--no-landlock");
    }
    if controls.toggle_tmpfs.is_active() {
        cmd.arg("--tmpfs");
    }
    if controls.toggle_share_pid.is_active() {
        cmd.arg("--share-pid");
    }
    if !controls.toggle_portals.is_active() {
        cmd.arg("--no-portals");
    }
    if controls.toggle_vpnify.is_active() {
        cmd.arg("--vpnify");
    }

    let shm_id = controls
        .combo_shm
        .active_id()
        .unwrap_or_else(|| "sandboxed".into());
    cmd.arg("--shm").arg(shm_id.as_str());

    let tmp_id = controls
        .combo_tmp
        .active_id()
        .unwrap_or_else(|| "sandboxed".into());
    cmd.arg("--tmp").arg(tmp_id.as_str());

    let bwrap_extra = controls.entry_bwrap.text().to_string();
    for piece in bwrap_extra.split_whitespace() {
        cmd.arg("--bwrap-arg").arg(piece);
    }

    let dbus_extra = controls.entry_dbus.text().to_string();
    for piece in dbus_extra.split_whitespace() {
        cmd.arg("--dbus-arg").arg(piece);
    }

    for bridge_flag in controls.bridges_widget.get_flags() {
        cmd.arg(bridge_flag);
    }
}

fn launch_sandbox(controls: &GuiControls) {
    let exe = controls.entry_exe.text().to_string();
    if exe.trim().is_empty() {
        return;
    }

    let mut cmd = Command::new("sb-run");

    let id_val = controls.entry_id.text().to_string();
    if !id_val.trim().is_empty() {
        cmd.arg("--id").arg(id_val.trim());
    }

    let net_id = controls
        .combo_net
        .active_id()
        .unwrap_or_else(|| "sandboxed".into());
    cmd.arg("--net").arg(net_id.as_str());

    append_subsystem_args(&mut cmd, controls);
    append_adv_args(&mut cmd, controls);

    for flag in controls.paths_widget.get_flags() {
        cmd.arg(flag);
    }

    cmd.arg("--").arg(exe);

    let _ = cmd.spawn();
}

fn main() {
    gtk4::init().expect("GTK4 init failed");
    apply_clean_theme();
    let main_loop = glib::MainLoop::new(None, false);

    let (win, controls) = build_gui_window();

    let btn_launch = gtk4::Button::with_label("Запустить в песочнице");
    btn_launch.add_css_class("suggested-action");

    let ml = main_loop.clone();
    btn_launch.connect_clicked(move |_| {
        launch_sandbox(&controls);
        ml.quit();
    });

    if let Some(root_box) = win.child().and_then(|c| c.downcast::<gtk4::Box>().ok()) {
        root_box.append(&btn_launch);
    }

    win.show();
    main_loop.run();
}
