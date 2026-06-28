//! Main window construction (libadwaita).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::iperf::{self, ClientConfig, Event, IpVersion, ServerConfig, Session};
use crate::vumeter::{format_speed, VuMeter};

pub fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("iperf3")
        .default_width(560)
        .default_height(680)
        .width_request(400)
        .height_request(560)
        .build();

    // View stack: Client / Server.
    let stack = adw::ViewStack::new();
    let client_page = build_client_page();
    let server_page = build_server_page();

    let p1 = stack.add_titled(&client_page, Some("client"), "Client");
    p1.set_icon_name(Some("network-transmit-symbolic"));
    let p2 = stack.add_titled(&server_page, Some("server"), "Server");
    p2.set_icon_name(Some("network-server-symbolic"));

    // Header with integrated view switcher.
    let header = adw::HeaderBar::new();
    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    header.set_title_widget(Some(&switcher));

    // Bottom switcher bar (for narrow windows / mobile).
    let switcher_bar = adw::ViewSwitcherBar::builder().stack(&stack).build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));
    toolbar.add_bottom_bar(&switcher_bar);

    window.set_content(Some(&toolbar));
    window
}

/// Creates a pre-configured numeric row (AdwSpinRow).
fn spin_row(title: &str, min: f64, max: f64, step: f64, value: f64) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(min, max, step);
    row.set_title(title);
    row.set_value(value);
    row
}

/// Creates a text entry row (AdwEntryRow), optionally with a subtitle hint.
fn entry_row(title: &str) -> adw::EntryRow {
    adw::EntryRow::builder().title(title).build()
}

fn build_client_page() -> gtk::Widget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    // --- Connection ---
    let group = adw::PreferencesGroup::new();
    group.set_title("Connection");

    let host_row = adw::EntryRow::builder().title("Server (host or IP)").build();
    host_row.set_text("127.0.0.1");
    let port_row = spin_row("Port", 1.0, 65535.0, 1.0, 5201.0);
    let dur_row = spin_row("Duration (s)", 1.0, 86400.0, 1.0, 10.0);

    group.add(&host_row);
    group.add(&port_row);
    group.add(&dur_row);
    root.append(&group);

    // --- Test options ---
    let test_group = adw::PreferencesGroup::new();
    test_group.set_title("Test");

    let par_row = spin_row("Parallel streams", 1.0, 128.0, 1.0, 1.0);
    let rev_row = adw::SwitchRow::builder()
        .title("Download mode (reverse)")
        .subtitle("Server sends to this machine")
        .build();
    let bidir_row = adw::SwitchRow::builder()
        .title("Bidirectional")
        .subtitle("Measure both directions at once (overrides reverse)")
        .build();
    let udp_row = adw::SwitchRow::builder()
        .title("UDP")
        .subtitle("Use UDP instead of TCP")
        .build();
    let bitrate_row = entry_row("Target bitrate");
    // EntryRow doesn't take subtitles; use placeholder-style hint via text.
    bitrate_row.set_tooltip_text(Some(
        "e.g. 100M, 1G. Empty = unlimited (TCP). UDP defaults to unlimited.",
    ));
    let omit_row = spin_row("Omit first seconds", 0.0, 60.0, 1.0, 0.0);

    test_group.add(&par_row);
    test_group.add(&rev_row);
    test_group.add(&bidir_row);
    test_group.add(&udp_row);
    test_group.add(&bitrate_row);
    test_group.add(&omit_row);
    root.append(&test_group);

    // --- Advanced (collapsible) ---
    let adv_group = adw::PreferencesGroup::new();
    let advanced = adw::ExpanderRow::builder()
        .title("Advanced options")
        .subtitle("Binding, buffers, TCP tuning")
        .build();

    let ip_row = adw::ComboRow::new();
    ip_row.set_title("IP version");
    let ip_model = gtk::StringList::new(&["Automatic", "IPv4 only", "IPv6 only"]);
    ip_row.set_model(Some(&ip_model));

    let bind_row = entry_row("Bind to local address");
    bind_row.set_tooltip_text(Some("Local interface/address to bind (-B)"));
    let window_row = entry_row("TCP window / socket buffer");
    window_row.set_tooltip_text(Some("e.g. 256K (-w)"));
    let length_row = entry_row("Buffer length");
    length_row.set_tooltip_text(Some("Read/write buffer size, e.g. 128K (-l)"));
    let mss_row = spin_row("Max segment size (MSS)", 0.0, 9000.0, 1.0, 0.0);
    let cong_row = entry_row("Congestion algorithm");
    cong_row.set_tooltip_text(Some("TCP only, e.g. cubic, bbr (-C)"));
    let nodelay_row = adw::SwitchRow::builder()
        .title("No delay")
        .subtitle("Disable Nagle's algorithm (-N)")
        .build();
    let zerocopy_row = adw::SwitchRow::builder()
        .title("Zero-copy")
        .subtitle("Use a zero-copy send method (-Z)")
        .build();

    advanced.add_row(&ip_row);
    advanced.add_row(&bind_row);
    advanced.add_row(&window_row);
    advanced.add_row(&length_row);
    advanced.add_row(&mss_row);
    advanced.add_row(&cong_row);
    advanced.add_row(&nodelay_row);
    advanced.add_row(&zerocopy_row);
    adv_group.add(&advanced);
    root.append(&adv_group);

    // --- VU-meter ---
    let vu = VuMeter::new();
    let frame = gtk::Frame::new(None);
    frame.add_css_class("card");
    frame.set_child(Some(vu.widget()));
    frame.set_vexpand(true);
    root.append(&frame);

    // --- Status + button ---
    let status = gtk::Label::new(Some("Ready."));
    status.add_css_class("dim-label");
    status.set_wrap(true);
    status.set_xalign(0.0);
    root.append(&status);

    let start_btn = gtk::Button::with_label("Start test");
    start_btn.add_css_class("suggested-action");
    start_btn.add_css_class("pill");
    start_btn.set_halign(gtk::Align::Center);
    root.append(&start_btn);

    // --- Shared state ---
    let session: Rc<RefCell<Option<Session>>> = Rc::new(RefCell::new(None));

    let click = {
        let session = session.clone();
        let vu = vu.clone();
        let start_btn = start_btn.clone();
        let status = status.clone();
        let host_row = host_row.clone();
        let port_row = port_row.clone();
        let dur_row = dur_row.clone();
        let par_row = par_row.clone();
        let rev_row = rev_row.clone();
        let bidir_row = bidir_row.clone();
        let udp_row = udp_row.clone();
        let bitrate_row = bitrate_row.clone();
        let omit_row = omit_row.clone();
        let ip_row = ip_row.clone();
        let bind_row = bind_row.clone();
        let window_row = window_row.clone();
        let length_row = length_row.clone();
        let mss_row = mss_row.clone();
        let cong_row = cong_row.clone();
        let nodelay_row = nodelay_row.clone();
        let zerocopy_row = zerocopy_row.clone();
        move |_btn: &gtk::Button| {
            // Already running? → stop.
            if session.borrow().is_some() {
                if let Some(s) = session.borrow_mut().take() {
                    s.stop();
                }
                vu.reset();
                start_btn.set_label("Start test");
                start_btn.remove_css_class("destructive-action");
                start_btn.add_css_class("suggested-action");
                status.set_text("Test stopped.");
                return;
            }

            let ip_version = match ip_row.selected() {
                1 => IpVersion::V4,
                2 => IpVersion::V6,
                _ => IpVersion::Auto,
            };
            let cfg = ClientConfig {
                host: host_row.text().trim().to_string(),
                port: port_row.value() as u16,
                duration: dur_row.value() as u32,
                parallel: par_row.value() as u32,
                reverse: rev_row.is_active(),
                bidir: bidir_row.is_active(),
                udp: udp_row.is_active(),
                bitrate: bitrate_row.text().trim().to_string(),
                omit: omit_row.value() as u32,
                ip_version,
                bind: bind_row.text().trim().to_string(),
                window: window_row.text().trim().to_string(),
                length: length_row.text().trim().to_string(),
                mss: mss_row.value() as u32,
                congestion: cong_row.text().trim().to_string(),
                no_delay: nodelay_row.is_active(),
                zerocopy: zerocopy_row.is_active(),
            };
            if cfg.host.is_empty() {
                status.set_text("Specify a server (host or IP).");
                return;
            }

            vu.reset();
            let (sess, rx) = iperf::run_client(cfg);
            *session.borrow_mut() = Some(sess);
            start_btn.set_label("Stop");
            start_btn.remove_css_class("suggested-action");
            start_btn.add_css_class("destructive-action");
            status.set_text("Connecting…");

            spawn_event_loop(
                rx,
                vu.clone(),
                status.clone(),
                start_btn.clone(),
                session.clone(),
                false,
            );
        }
    };
    start_btn.connect_clicked(click);

    root.upcast()
}

fn build_server_page() -> gtk::Widget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    let group = adw::PreferencesGroup::new();
    group.set_title("Server");
    let port_row = spin_row("Listen port", 1.0, 65535.0, 1.0, 5201.0);
    let bind_row = entry_row("Bind to local address");
    bind_row.set_tooltip_text(Some("Local interface/address to listen on (-B)"));
    let oneoff_row = adw::SwitchRow::builder()
        .title("One-off")
        .subtitle("Serve a single client then exit (-1)")
        .build();
    group.add(&port_row);
    group.add(&bind_row);
    group.add(&oneoff_row);
    root.append(&group);

    let vu = VuMeter::new();
    let frame = gtk::Frame::new(None);
    frame.add_css_class("card");
    frame.set_child(Some(vu.widget()));
    frame.set_vexpand(true);
    root.append(&frame);

    let status = gtk::Label::new(Some("Stopped."));
    status.add_css_class("dim-label");
    status.set_wrap(true);
    status.set_xalign(0.0);
    root.append(&status);

    let start_btn = gtk::Button::with_label("Start server");
    start_btn.add_css_class("suggested-action");
    start_btn.add_css_class("pill");
    start_btn.set_halign(gtk::Align::Center);
    root.append(&start_btn);

    let session: Rc<RefCell<Option<Session>>> = Rc::new(RefCell::new(None));

    let click = {
        let session = session.clone();
        let vu = vu.clone();
        let start_btn = start_btn.clone();
        let status = status.clone();
        let port_row = port_row.clone();
        let bind_row = bind_row.clone();
        let oneoff_row = oneoff_row.clone();
        move |_btn: &gtk::Button| {
            if session.borrow().is_some() {
                if let Some(s) = session.borrow_mut().take() {
                    s.stop();
                }
                vu.reset();
                start_btn.set_label("Start server");
                start_btn.remove_css_class("destructive-action");
                start_btn.add_css_class("suggested-action");
                status.set_text("Stopped.");
                return;
            }

            let cfg = ServerConfig {
                port: port_row.value() as u16,
                bind: bind_row.text().trim().to_string(),
                one_off: oneoff_row.is_active(),
            };
            vu.reset();
            let (sess, rx) = iperf::run_server(cfg);
            *session.borrow_mut() = Some(sess);
            start_btn.set_label("Stop server");
            start_btn.remove_css_class("suggested-action");
            start_btn.add_css_class("destructive-action");
            status.set_text(&format!("Listening on port {}…", port_row.value() as u16));

            spawn_event_loop(
                rx,
                vu.clone(),
                status.clone(),
                start_btn.clone(),
                session.clone(),
                true,
            );
        }
    };
    start_btn.connect_clicked(click);

    root.upcast()
}

/// Runs the loop that consumes events from the thread and updates the UI.
fn spawn_event_loop(
    rx: async_channel::Receiver<Event>,
    vu: VuMeter,
    status: gtk::Label,
    start_btn: gtk::Button,
    session: Rc<RefCell<Option<Session>>>,
    is_server: bool,
) {
    glib::spawn_future_local(async move {
        // Recordamos el último error de stderr para no taparlo con el mensaje
        // genérico de "terminó con código N".
        let mut last_error: Option<String> = None;
        while let Ok(ev) = rx.recv().await {
            match ev {
                Event::Interval { mbps } => {
                    vu.set_target(mbps);
                    if !is_server {
                        let (v, u) = format_speed(mbps);
                        status.set_text(&format!("Measuring… {v} {u}"));
                    }
                }
                Event::Summary {
                    sender_mbps,
                    receiver_mbps,
                } => {
                    let (sv, su) = format_speed(sender_mbps);
                    let (rv, ru) = format_speed(receiver_mbps);
                    status.set_text(&format!(
                        "Result — send: {sv} {su} · receive: {rv} {ru}"
                    ));
                }
                Event::ClientConnected { peer } => {
                    status.set_text(&format!("Client connected: {peer} — measuring…"));
                }
                Event::ClientDisconnected => {
                    vu.reset();
                }
                Event::Listening => {
                    if is_server {
                        status.set_text("Waiting for client connections…");
                    }
                }
                Event::Error(msg) => {
                    // Nos quedamos con el primer error (el más informativo).
                    if last_error.is_none() {
                        last_error = Some(msg.clone());
                    }
                    status.set_text(&msg);
                }
                Event::Log(_) => {}
                Event::Finished(code) => {
                    *session.borrow_mut() = None;
                    vu.reset();
                    if !is_server {
                        start_btn.set_label("Start test");
                    } else {
                        start_btn.set_label("Start server");
                    }
                    start_btn.remove_css_class("destructive-action");
                    start_btn.add_css_class("suggested-action");
                    if code != 0 && code != -1 {
                        // Si iperf3 ya escribió un motivo por stderr, lo dejamos
                        // visible en lugar del mensaje genérico.
                        if let Some(err) = &last_error {
                            status.set_text(&format!("iperf3 error: {err}"));
                        } else {
                            status.set_text(&format!("iperf3 terminated (code {code})."));
                        }
                    }
                    break;
                }
            }
        }
    });
}
