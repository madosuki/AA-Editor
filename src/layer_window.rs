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

    pub fn init<F>(&self, window_title: std::string::String, width: i32, height: i32, on_close: F)
    where
        F: Fn() + 'static,
    {
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

        let close_button = gtk::Button::with_label("×");
        close_button.add_css_class("layer-close-button");
        close_button.set_focusable(false);
        {
            let canvas = self.canvas.clone();
            let frame = self.frame.clone().upcast::<gtk::Widget>();
            close_button.connect_clicked(move |_| {
                canvas.remove_layer(&frame);
                on_close();
            });
        }

        header.append(&title);
        header.append(&close_button);

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
        self.frame.append(&self.resize_handle());
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

    pub fn widget(&self) -> gtk::Widget {
        self.frame.clone().upcast()
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

            .layer-close-button {
                padding: 0 6px;
                min-width: 24px;
                min-height: 20px;
            }

            .layer-text {
                font-family: Monapo, 'MS PGothic', sans-serif;
                font-size: 16px;
                line-height: 18px;
            }

            .layer-resize-row {
                padding: 0 2px 2px 0;
            }

            .layer-resize-handle {
                background: #9ca3af;
                min-width: 14px;
                min-height: 14px;
            }",
        );

        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    fn resize_handle(&self) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.add_css_class("layer-resize-row");
        row.set_halign(gtk::Align::Fill);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);

        let handle = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        handle.add_css_class("layer-resize-handle");
        handle.set_cursor_from_name(Some("nwse-resize"));

        row.append(&spacer);
        row.append(&handle);
        row
    }

    pub fn has_widget(&self, widget: &gtk::Widget) -> bool {
        self.frame.clone().upcast::<gtk::Widget>() == *widget
    }
}
