use gtk4 as gtk;

use gtk::prelude::{
    ApplicationWindowExt, GtkWindowExt, WidgetExt,
};
use gtk::{Application, ApplicationWindow};

use anyhow::Result;


struct MainWindow {
    window: ApplicationWindow,
    v_box: gtk::Box,
    view_window: gtk::ScrolledWindow,
}

fn update_window_title(window: &gtk::ApplicationWindow, title_text: &str) {
    let Some(_) = window.title() else {
        return;
    };

    let new_title = format!("AA Editor: {}", title_text);
    window.set_title(Some(&new_title));
}

fn fullscreen(window: &gtk::ApplicationWindow, _pages_bar: &gtk::ProgressBar) {
    if window.is_fullscreen() {
        window.unfullscreen();
        window.set_show_menubar(true);
    } else {
        window.fullscreen();
        window.set_show_menubar(false);
    }
}

impl MainWindow {
    fn new(_app: &Application) -> Self {
        let window_ui_src = include_str!("window.ui");

        let builder = gtk::Builder::new();
        let _ = builder.add_from_string(window_ui_src);

        let win = builder.object("window").unwrap();


        let result = MainWindow {
            window: win,
            v_box: gtk::Box::new(gtk::Orientation::Vertical, 1),
            view_window: gtk::ScrolledWindow::new(),
        };

        result
    }

    fn init(&self, app: &Application, width: i32, height: i32) -> Result<()> {
        self.window.set_title(Some("Simple Comics Viewer"));
        self.window.set_default_size(width, height);
        self.window.set_show_menubar(true);

        let _window = &self.window;

        self.v_box.set_halign(gtk::Align::Fill);
        self.v_box.set_valign(gtk::Align::Fill);
        self.v_box.set_hexpand(true);
        self.v_box.set_vexpand(true);

        self.window.set_application(Some(app));
        self.window.set_child(Some(&self.v_box));

        Ok(())
    }

    fn run(&self) {
        self.window.show();
    }
}

pub fn activate(app: &Application) {
    let main = MainWindow::new(app);
    match main.init(app, 1024, 768) {
        Ok(_) => {
            main.run();
        }
        Err(e) => {
            println!("{}", e);
        }
    }
}
