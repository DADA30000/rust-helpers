use gtk4::prelude::*;
use std::env;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use system_ui_helpers::apply_clean_theme;

unsafe extern "C" {
    fn getuid() -> u32;
}

fn is_prefix_running(prefix: &str) -> bool {
    let unit = format!("umu-pfx-{prefix}.scope");
    if let Ok(status) = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", &unit])
        .status()
        && status.success()
    {
        return true;
    }

    // Secondary check: is overlay mounted in runtime dir?
    let uid = unsafe { getuid() };
    let mount_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{uid}"));
    let pfx_mount = format!("{mount_dir}/umu-pfx/{prefix}");
    if let Ok(status) = Command::new("mountpoint").args(["-q", &pfx_mount]).status()
        && status.success()
    {
        return true;
    }

    false
}

fn stop_prefix(prefix: &str) {
    let unit = format!("umu-pfx-{prefix}.scope");
    Command::new("systemctl")
        .args(["--user", "stop", &unit])
        .status()
        .ok();

    // Wait up to 3 seconds for unit to stop
    for _ in 0..30 {
        if !is_prefix_running(prefix) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Secondary fallback cleanup if still mounted
    let uid = unsafe { getuid() };
    let mount_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{uid}"));
    let pfx_mount = format!("{mount_dir}/umu-pfx/{prefix}");
    Command::new("umount")
        .args(["-l", &pfx_mount])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: manage-running-prefix <prefix_name>");
        std::process::exit(0);
    }
    let prefix = args[1].clone();

    // If prefix is not currently running, exit 0 immediately
    if !is_prefix_running(&prefix) {
        std::process::exit(0);
    }

    gtk4::init().expect("GTK4 init failed");
    apply_clean_theme();
    let main_loop = glib::MainLoop::new(None, false);

    let win = gtk4::Window::builder()
        .title("Префикс уже запущен - UMU")
        .default_width(440)
        .resizable(false)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);
    win.set_child(Some(&vbox));

    let lbl_msg = gtk4::Label::builder()
        .label(format!(
            "<b>Префикс '{prefix}' сейчас работает!</b>\n\nВ данный момент префикс используется другим процессом.\nВы можете принудительно завершить его или прервать текущий запуск."
        ))
        .use_markup(true)
        .xalign(0.0)
        .wrap(true)
        .build();
    vbox.append(&lbl_msg);

    let bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    bbox.set_halign(gtk4::Align::End);
    bbox.set_margin_top(8);

    let btn_stop = gtk4::Button::with_label("Закрыть префикс");
    btn_stop.add_css_class("destructive-action");

    let btn_cancel = gtk4::Button::with_label("Прервать запуск");

    bbox.append(&btn_cancel);
    bbox.append(&btn_stop);
    vbox.append(&bbox);

    let _ml_stop = main_loop.clone();
    let p_stop = prefix;
    btn_stop.connect_clicked(move |_| {
        stop_prefix(&p_stop);
        Command::new("notify-send")
            .args([
                "Префикс закрыт",
                &format!("Работа префикса '{p_stop}' завершена"),
            ])
            .spawn()
            .ok();
        // Exit 0 allows wrapper to continue and start fresh instance
        std::process::exit(0);
    });

    let ml_cancel = main_loop.clone();
    btn_cancel.connect_clicked(move |_| {
        ml_cancel.quit();
        // Exit 1 tells wrapper to abort
        std::process::exit(1);
    });

    let ml_win = main_loop.clone();
    win.connect_close_request(move |_| {
        ml_win.quit();
        std::process::exit(1);
    });

    win.present();
    main_loop.run();
}
