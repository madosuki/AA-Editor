use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, gdk};
use gtk4 as gtk;

use crate::canvas::Canvas;

#[derive(Clone)]
pub struct LayerWindow {
    title: Rc<RefCell<std::string::String>>,
    x: Rc<Cell<i32>>,
    y: Rc<Cell<i32>>,
    scrolled_window: gtk::ScrolledWindow,
    frame: gtk::Box,
    canvas: Canvas,
}

impl LayerWindow {
    pub fn new(canvas: &Canvas) -> Self {
        Self {
            title: Rc::new(RefCell::new("Layer 1".to_owned())),
            x: Rc::new(Cell::new(0)),
            y: Rc::new(Cell::new(0)),
            scrolled_window: gtk::ScrolledWindow::new(),
            frame: gtk::Box::new(gtk::Orientation::Vertical, 0),
            canvas: canvas.clone(),
        }
    }

    pub fn init(&self, window_title: std::string::String, width: i32, height: i32) {
        Self::install_style();

        self.frame.add_css_class("layer-panel");
        self.frame.set_size_request(width, height);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("layer-header");
        header.set_hexpand(true);

        *self.title.borrow_mut() = window_title;
        let title = gtk::Label::new(Some(self.title.borrow().as_str()));
        title.set_xalign(0.0);
        title.set_hexpand(true);

        header.append(&title);

        let text_view = gtk::TextView::new();
        text_view.add_css_class("layer-text");
        text_view.set_editable(true);
        text_view.set_wrap_mode(gtk::WrapMode::None);
        text_view.set_left_margin(8);
        text_view.set_right_margin(8);
        text_view.set_top_margin(8);
        text_view.set_bottom_margin(8);
        text_view.set_hexpand(true);
        text_view.set_vexpand(true);
        text_view
            .buffer()
            .set_text("Layer preview\n\nAA text layer area");

        self.scrolled_window
            .set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        self.scrolled_window.set_child(Some(&text_view));
        self.scrolled_window.set_hexpand(true);
        self.scrolled_window.set_vexpand(true);

        self.frame.append(&header);
        self.frame.append(&self.scrolled_window);
    }

    pub fn attach_to(&self) {
        self.frame.set_halign(gtk::Align::Start);
        self.frame.set_valign(gtk::Align::Start);
        self.canvas.put_layer(
            &self.frame.clone().upcast::<gtk::Widget>(),
            self.x.get() as f64,
            self.y.get() as f64,
        );

        self.frame.show();
    }

    pub fn remove_from_parent(&self) {
        self.canvas
            .remove_layer(&self.frame.clone().upcast::<gtk::Widget>());
    }

    fn install_style() {
        let Some(display) = gdk::Display::default() else {
            return;
        };

        let provider = CssProvider::new();
        provider.load_from_data(
            ".layer-panel {
                background: white;
                border: 1px solid #2f3437;
            }

            .layer-header {
                background: #e5e7eb;
                border-bottom: 1px solid #2f3437;
                padding: 6px 8px;
                font-weight: bold;
            }

            .layer-text {
                font-family: Monapo, 'MS PGothic', sans-serif;
                font-size: 16px;
                line-height: 18px;
            }",
        );

        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
