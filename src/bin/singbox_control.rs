use gtk4::prelude::*;
use serde_json::Value;
use std::thread;
use system_ui_helpers::*;

const API_BASE: &str = "http://127.0.0.1:9090";

fn api_get(endpoint: &str) -> Option<Value> {
    ureq::get(&format!("{}{}", API_BASE, endpoint))
        .timeout(std::time::Duration::from_secs(2))
        .call()
        .ok()?
        .into_json()
        .ok()
}

fn api_put(endpoint: &str, payload: Value) -> bool {
    ureq::put(&format!("{}{}", API_BASE, endpoint))
        .send_json(payload)
        .is_ok()
}

#[derive(Clone)]
struct ProxyNode {
    name: String,
    is_active: bool,
}

fn main() {
    gtk4::init().expect("GTK4 init failed");
    apply_clean_theme();
    let main_loop = glib::MainLoop::new(None, false);

    let win = gtk4::Window::builder()
        .title("Proxy Selector")
        .default_width(420)
        .default_height(560)
        .resizable(false)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    vbox.set_margin_top(15);
    vbox.set_margin_bottom(15);
    vbox.set_margin_start(15);
    vbox.set_margin_end(15);
    win.set_child(Some(&vbox));

    let header_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let icon_img = gtk4::Image::from_icon_name("network-workgroup-symbolic");
    icon_img.set_pixel_size(32);
    icon_img.set_valign(gtk4::Align::Center);
    header_box.append(&icon_img);

    let lbl_active = gtk4::Label::new(None);
    lbl_active.set_markup("<span size='large'>Active proxy: <b>Connecting...</b></span>");
    lbl_active.set_xalign(0.0);
    lbl_active.set_hexpand(true);
    header_box.append(&lbl_active);
    vbox.append(&header_box);
    vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let hbox_final = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let lbl_final_info = gtk4::Label::builder()
        .label("<b>Route unmatched traffic to:</b>")
        .use_markup(true)
        .xalign(0.0)
        .hexpand(true)
        .build();
    hbox_final.append(&lbl_final_info);

    let lbl_final_state = gtk4::Label::builder()
        .label("<span weight='bold' color='gray'>direct</span>")
        .use_markup(true)
        .xalign(1.0)
        .build();
    hbox_final.append(&lbl_final_state);

    let switch_final = gtk4::Switch::new();
    switch_final.set_valign(gtk4::Align::Center);
    hbox_final.append(&switch_final);
    vbox.append(&hbox_final);

    vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text("Filter proxies...")
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

    let action_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    action_box.set_halign(gtk4::Align::End);
    let btn_test = gtk4::Button::with_label("Test All Latencies");
    let btn_close = gtk4::Button::with_label("Close");

    btn_test.set_focusable(false);
    btn_close.set_focusable(false);

    action_box.append(&btn_test);
    action_box.append(&btn_close);
    vbox.append(&action_box);

    let w_close = win.clone();
    btn_close.connect_clicked(move |_| w_close.close());

    let (tx, rx) = std::sync::mpsc::channel::<(Vec<ProxyNode>, String, String)>();
    let (delay_tx, delay_rx) = std::sync::mpsc::channel::<(String, i32)>();

    let lbl_clone = lbl_active.clone();
    let lb_clone = listbox.clone();
    let sf_clone = switch_final.clone();
    let sfl_clone = lbl_final_state.clone();
    let search_clone = search_entry.clone();

    listbox.set_filter_func(move |row| {
        let text = search_clone.text().to_lowercase();
        if text.is_empty() {
            return true;
        }
        let name = row.widget_name().to_lowercase();
        name.contains(&text)
    });

    let lb_search = listbox.clone();
    search_entry.connect_search_changed(move |_| {
        lb_search.invalidate_filter();
    });

    let selector_name_global = std::rc::Rc::new(std::cell::RefCell::new("proxy".to_string()));
    let sel_ref = selector_name_global.clone();
    let btn_test_rx = btn_test.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        if let Ok((nodes, active, final_mode)) = rx.try_recv() {
            lbl_clone.set_markup(&format!(
                "<span size='large'>Active proxy: <span color='#2ecc71'><b>{}</b></span></span>",
                active
            ));

            let is_proxy = final_mode == "proxy";
            sf_clone.set_active(is_proxy);
            sfl_clone.set_markup(if is_proxy {
                "<span weight='bold' color='#3498db'>proxy</span>"
            } else {
                "<span weight='bold' color='#95a5a6'>direct</span>"
            });

            while let Some(child) = lb_clone.first_child() {
                lb_clone.remove(&child);
            }

            for node in nodes {
                let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
                row_box.set_margin_top(8);
                row_box.set_margin_bottom(8);
                row_box.set_margin_start(8);
                row_box.set_margin_end(8);

                let icon = gtk4::Image::from_icon_name(if node.is_active {
                    "emblem-ok-symbolic"
                } else {
                    "network-vpn-symbolic"
                });
                icon.set_valign(gtk4::Align::Center);

                let label_text = if node.is_active {
                    format!("<b>{}</b>", node.name)
                } else {
                    node.name.clone()
                };
                let label = gtk4::Label::builder()
                    .xalign(0.0)
                    .hexpand(true)
                    .use_markup(true)
                    .label(&label_text)
                    .build();

                let delay_lbl = gtk4::Label::builder().xalign(1.0).opacity(0.6).build();

                row_box.append(&icon);
                row_box.append(&label);
                row_box.append(&delay_lbl);

                let row = gtk4::ListBoxRow::new();
                row.set_selectable(false);
                row.set_focusable(false);
                row.set_activatable(true);
                row.set_widget_name(&node.name);
                row.set_child(Some(&row_box));
                lb_clone.append(&row);
            }
        }

        while let Ok((node_name, delay)) = delay_rx.try_recv() {
            if node_name == "___DONE___" {
                btn_test_rx.set_sensitive(true);
                btn_test_rx.set_label("Test All Latencies");
                continue;
            }

            let mut child = lb_clone.first_child();
            while let Some(row) = child {
                if row.widget_name() == node_name {
                    if let Some(bx) = row.downcast_ref::<gtk4::ListBoxRow>().unwrap().child() {
                        if let Some(dl) = bx.last_child() {
                            let dl = dl.downcast_ref::<gtk4::Label>().unwrap();
                            if delay > 0 {
                                dl.set_markup(&format!(
                                    "<span color='#2ecc71'><b>{} ms</b></span>",
                                    delay
                                ));
                            } else {
                                dl.set_markup("<span color='#e74c3c'>timeout</span>");
                            }
                        }
                    }
                    break;
                }
                child = row.next_sibling();
            }
        }
        glib::ControlFlow::Continue
    });

    let fetch_data = move |tx: std::sync::mpsc::Sender<_>| {
        thread::spawn(move || {
            if let Some(data) = api_get("/proxies") {
                if let Some(proxies) = data.get("proxies").and_then(|p| p.as_object()) {
                    let mut sel_name = "proxy";
                    if !proxies.contains_key("proxy") && proxies.contains_key("out") {
                        sel_name = "out";
                    }

                    if let Some(proxy) = proxies.get(sel_name).and_then(|p| p.as_object()) {
                        let active = proxy
                            .get("now")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        let final_now = proxies
                            .get("final-toggle")
                            .and_then(|f| f.get("now"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("direct")
                            .to_string();

                        if let Some(all) = proxy.get("all").and_then(|a| a.as_array()) {
                            let nodes: Vec<ProxyNode> = all
                                .iter()
                                .map(|n| {
                                    let name = n.as_str().unwrap_or("").to_string();
                                    ProxyNode {
                                        is_active: name == active,
                                        name,
                                    }
                                })
                                .collect();
                            tx.send((nodes, active, final_now)).ok();
                        }
                    }
                }
            }
        });
    };

    let tx_clone = tx.clone();
    fetch_data(tx_clone);

    let tx_click = tx.clone();
    let s_ref = sel_ref.clone();
    listbox.connect_row_activated(move |_, row| {
        let target = row.widget_name().to_string();
        let sn = s_ref.borrow().clone();
        let tx_c = tx_click.clone();
        thread::spawn(move || {
            if api_put(
                &format!("/proxies/{}", sn),
                serde_json::json!({"name": target}),
            ) {
                fetch_data(tx_c);
            }
        });
    });

    let sfl_clone2 = lbl_final_state.clone();
    switch_final.connect_state_set(move |_, state| {
        let target = if state { "proxy" } else { "direct" };
        sfl_clone2.set_markup(if state {
            "<span weight='bold' color='#3498db'>proxy</span>"
        } else {
            "<span weight='bold' color='#95a5a6'>direct</span>"
        });
        thread::spawn(move || {
            api_put("/proxies/final-toggle", serde_json::json!({"name": target}));
        });
        glib::Propagation::Proceed
    });

    let lb_test = listbox.clone();
    let bt_clone = btn_test.clone();

    btn_test.connect_clicked(move |_| {
        bt_clone.set_sensitive(false);
        bt_clone.set_label("Testing...");
        let mut nodes = Vec::new();
        let mut child = lb_test.first_child();
        while let Some(row) = child {
            nodes.push(row.widget_name().to_string());
            child = row.next_sibling();
        }
        let d_tx = delay_tx.clone();

        thread::spawn(move || {
            for node in nodes {
                let url = format!(
                    "/proxies/{}/delay?timeout=3000&url=http%3A%2F%2Fwww.gstatic.com%2Fgenerate_204",
                    urlencoding::encode(&node)
                );
                let delay = if let Some(res) = api_get(&url) {
                    res.get("delay").and_then(|d| d.as_i64()).unwrap_or(-2) as i32
                } else {
                    -2
                };
                d_tx.send((node, delay)).ok();
            }
            d_tx.send(("___DONE___".to_string(), 0)).ok();
        });
    });

    let ml = main_loop.clone();
    win.connect_close_request(move |_| {
        ml.quit();
        glib::Propagation::Proceed
    });

    search_entry.grab_focus();
    win.present();
    main_loop.run();
}
