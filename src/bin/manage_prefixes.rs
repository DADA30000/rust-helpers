use gtk4::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use system_ui_helpers::*;

fn main() {
    gtk4::init().expect("GTK4 init failed");
    apply_clean_theme();
    let main_loop = glib::MainLoop::new(None, false);

    let win = gtk4::Window::builder()
        .title("UMU Prefix Manager")
        .default_width(500)
        .default_height(350)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_top(10);
    vbox.set_margin_bottom(10);
    vbox.set_margin_start(10);
    vbox.set_margin_end(10);
    win.set_child(Some(&vbox));

    let lbl = gtk4::Label::builder()
        .label("<b>Выберите префикс для настройки или управления:</b>")
        .use_markup(true)
        .xalign(0.0)
        .yalign(0.5)
        .build();
    vbox.append(&lbl);

    let listbox = gtk4::ListBox::new();
    listbox.set_selection_mode(gtk4::SelectionMode::Single);
    listbox.set_can_focus(true);
    let scroll = gtk4::ScrolledWindow::builder()
        .child(&listbox)
        .vexpand(true)
        .build();
    vbox.append(&scroll);

    let get_umu_dir = || -> PathBuf {
        let home = env::var("HOME").unwrap_or_else(|_| ".".into());
        let dir = PathBuf::from(home).join(".umu");
        fs::create_dir_all(&dir).ok();
        fs::create_dir_all(dir.join("default")).ok();
        dir
    };

    let populate = Rc::new(move |lb: &gtk4::ListBox| {
        while let Some(child) = lb.first_child() {
            lb.remove(&child);
        }
        let umu_dir = get_umu_dir();
        if let Ok(entries) = fs::read_dir(umu_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                if path.is_dir() && name != "steamrt3" && name != "umu" {
                    let row = gtk4::ListBoxRow::new();
                    row.set_widget_name(&format!("{}|{}", name, path.to_string_lossy()));

                    let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
                    row_box.set_margin_top(8);
                    row_box.set_margin_bottom(8);
                    row_box.set_margin_start(10);
                    row_box.set_margin_end(10);

                    let lbl_name = gtk4::Label::builder()
                        .label(&name)
                        .xalign(0.0)
                        .hexpand(true)
                        .build();

                    row_box.append(&lbl_name);
                    row.set_child(Some(&row_box));
                    lb.append(&row);
                }
            }
        }
    });

    populate(&listbox);

    let bbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    bbox.set_halign(gtk4::Align::End);
    let btn_winetricks = gtk4::Button::with_label("Winetricks");
    let btn_open = gtk4::Button::with_label("Открыть папку");
    let btn_delete = gtk4::Button::with_label("Удалить");
    let btn_close = gtk4::Button::with_label("Закрыть");

    btn_winetricks.set_focusable(false);
    btn_winetricks.set_can_focus(false);
    btn_open.set_focusable(false);
    btn_open.set_can_focus(false);
    btn_delete.set_focusable(false);
    btn_delete.set_can_focus(false);
    btn_close.set_focusable(false);
    btn_close.set_can_focus(false);

    bbox.append(&btn_winetricks);
    bbox.append(&btn_open);
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

    let gs1 = get_selected.clone();
    btn_winetricks.connect_clicked(move |_| {
        if let Some((_, path)) = gs1() {
            Command::new("umu-run-wrapper")
                .args(["winetricks", "--gui"])
                .env("WINEPREFIX", path)
                .spawn()
                .ok();
        }
    });

    let gs2 = get_selected.clone();
    btn_open.connect_clicked(move |_| {
        if let Some((_, path)) = gs2() {
            let drive_c = PathBuf::from(path).join("drive_c");
            fs::create_dir_all(&drive_c).ok();
            Command::new("xdg-open").arg(drive_c).spawn().ok();
        }
    });

    let w_del = win.clone();
    let gs3 = get_selected.clone();
    let pop_del = populate.clone();
    let lb_del = listbox.clone();
    btn_delete.connect_clicked(move |_| {
        if let Some((name, path)) = gs3() {
            if name == "default" {
                let err_dlg = gtk4::MessageDialog::builder()
                    .transient_for(&w_del)
                    .modal(true)
                    .message_type(gtk4::MessageType::Error)
                    .buttons(gtk4::ButtonsType::Ok)
                    .text("Нельзя удалить префикс по умолчанию ('default')!")
                    .build();
                err_dlg.connect_response(|d, _| d.destroy());
                err_dlg.present();
                return;
            }

            let confirm = gtk4::MessageDialog::builder()
                .transient_for(&w_del)
                .modal(true)
                .message_type(gtk4::MessageType::Question)
                .buttons(gtk4::ButtonsType::YesNo)
                .text(format!(
                    "Вы уверены, что хотите полностью удалить префикс '{}'? Все установленные туда игры и сохранения будут утеряны!",
                    name
                ))
                .build();

            let p_c = pop_del.clone();
            let lb_c = lb_del.clone();
            let w_c = w_del.clone();
            confirm.connect_response(move |d, res| {
                if res == gtk4::ResponseType::Yes {
                    if let Err(e) = fs::remove_dir_all(&path) {
                        let e_dlg = gtk4::MessageDialog::builder()
                            .transient_for(&w_c)
                            .modal(true)
                            .message_type(gtk4::MessageType::Error)
                            .buttons(gtk4::ButtonsType::Ok)
                            .text(format!("Ошибка удаления префикса: {}", e))
                            .build();
                        e_dlg.connect_response(|ed, _| ed.destroy());
                        e_dlg.present();
                    } else {
                        p_c(&lb_c);
                        Command::new("notify-send")
                            .args(["Prefix Deleted", &format!("Удален префикс {}", name)])
                            .spawn()
                            .ok();
                    }
                }
                d.destroy();
            });
            confirm.present();
        }
    });

    let ml_win = main_loop.clone();
    win.connect_close_request(move |_| {
        ml_win.quit();
        glib::Propagation::Proceed
    });

    listbox.grab_focus();
    win.present();
    main_loop.run();
}
