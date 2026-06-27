//! iperf_rust — frontend gráfico de iperf3 (GTK4 + libadwaita).

mod iperf;
mod vumeter;
mod window;

use adw::prelude::*;

const APP_ID: &str = "io.github.iperf_rust";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_startup(|_| {
        // Permite encontrar el icono al ejecutar desde el repositorio sin
        // instalar (busca data/icons junto al cwd o al binario). Una vez
        // instalado en el sistema, el tema hicolor lo encuentra solo.
        if let Some(display) = gtk::gdk::Display::default() {
            let theme = gtk::IconTheme::for_display(&display);
            for path in icon_search_paths() {
                theme.add_search_path(path);
            }
        }
        gtk::Window::set_default_icon_name(APP_ID);
    });

    app.connect_activate(|app| {
        let win = window::build_window(app);
        win.set_icon_name(Some(APP_ID));
        win.present();
    });
    app.run()
}

/// Rutas donde buscar `data/icons` durante el desarrollo (sin instalar).
fn icon_search_paths() -> Vec<std::path::PathBuf> {
    let mut paths = vec![std::path::PathBuf::from("data/icons")];
    if let Ok(exe) = std::env::current_exe() {
        // target/<perfil>/iperf_rust → subir hasta la raíz del proyecto.
        if let Some(root) = exe.ancestors().nth(3) {
            paths.push(root.join("data/icons"));
        }
    }
    paths
}
