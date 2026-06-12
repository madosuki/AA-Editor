mod layer_window;
mod main_window;
mod project_file;
mod settings;

use gtk::Application;
use gtk::gio::prelude::{ApplicationExt, ApplicationExtManual};
use gtk4 as gtk;

fn main() -> gtk::glib::ExitCode {
    if let Err(error) = settings::Settings::load_or_create() {
        eprintln!("{error}");
    }

    let app_id_str: &str = "com.aa_editor";
    let app = Application::builder().application_id(app_id_str).build();

    app.connect_activate(main_window::activate);
    app.run()
}
