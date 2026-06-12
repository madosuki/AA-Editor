use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION};
use gtk4 as gtk;

fn translate_transform(x: f64, y: f64) -> gtk::gsk::Transform {
    gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(x as f32, y as f32))
}

pub struct LayerWindow {
    title: Rc<RefCell<std::string::String>>,
    x: Rc<Cell<i32>>,
    y: Rc<Cell<i32>>,
    start_x_for_drag: Rc<Cell<f64>>,
    start_y_for_drag: Rc<Cell<f64>>,
    scrolled_window: gtk::ScrolledWindow,
    fixed: gtk::Fixed,
    frame: gtk::Box,
}

impl LayerWindow {
    pub fn new() -> Self {
        Self {
            title: Rc::new(RefCell::new("Layer 1".to_owned())),
            x: Rc::new(Cell::new(48)),
            y: Rc::new(Cell::new(48)),
            start_x_for_drag: Rc::new(Cell::new(48.0)),
            start_y_for_drag: Rc::new(Cell::new(48.0)),
            scrolled_window: gtk::ScrolledWindow::new(),
            fixed: gtk::Fixed::new(),
            frame: gtk::Box::new(gtk::Orientation::Vertical, 0),
        }
    }

    fn set_draggable(&self, widget: &impl IsA<gtk::Widget>) {
        let drag = gtk4::GestureDrag::new();
        {
            let x = self.x.clone();
            let y = self.y.clone();
            let start_x = self.start_x_for_drag.clone();
            let start_y = self.start_y_for_drag.clone();
            drag.connect_drag_begin(move |_, _, _| {
                start_x.set(x.get() as f64);
                start_y.set(y.get() as f64);
            });

        }
        {
            let fixed_cloned = self.fixed.clone();
            let frame_cloned = self.frame.clone();
            let start_x = self.start_x_for_drag.clone();
            let start_y = self.start_y_for_drag.clone();

            drag.connect_drag_update(move |_, offset_x, offset_y| {
                let new_x = start_x.get() + offset_x;
                let new_y = start_y.get() + offset_y;

                let final_x = new_x.max(0.0);
                let final_y = new_y.max(0.0);
                let transform = translate_transform(final_x, final_y);

                fixed_cloned.set_child_transform(&frame_cloned, Some(&transform));
            });
        }
        {
            let fixed_cloned = self.fixed.clone();
            let frame_cloned = self.frame.clone();

            let x = self.x.clone();
            let y = self.y.clone();
            let start_x = self.start_x_for_drag.clone();
            let start_y = self.start_y_for_drag.clone();

            drag.connect_drag_end(move |_, offset_x, offset_y| {
                let tmp_x = start_x.get() as f64 + offset_x;
                let tmp_y = start_y.get() as f64 + offset_y;
                let final_x = tmp_x.max(0.0);
                let final_y = tmp_y.max(0.0);
                x.set(final_x.round() as i32);
                y.set(final_y.round() as i32);

                fixed_cloned.set_child_transform(&frame_cloned, None);
                fixed_cloned.move_(&frame_cloned, final_x, final_y);
            });
        }

        widget.add_controller(drag);
    }

    pub fn init(&self, window_title: std::string::String, width: i32, height: i32) {
        Self::install_style();

        self.fixed.add_css_class("layer-fixed");
        self.fixed.set_size_request(width, height);
        self.fixed.set_hexpand(true);
        self.fixed.set_vexpand(true);

        self.frame.add_css_class("layer-panel");
        self.frame.set_size_request(640, 480);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("layer-header");
        header.set_hexpand(true);

        self.title.borrow_mut().push_str(&window_title);
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
        self.scrolled_window.set_child(Some(&self.fixed));
        self.scrolled_window.set_hexpand(true);
        self.scrolled_window.set_vexpand(true);

        self.frame.append(&header);
        self.frame.append(&text_view);
        self.fixed
            .put(&self.frame, self.x.get() as f64, self.y.get() as f64);

        self.set_draggable(&self.fixed);
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.scrolled_window
    }

    fn install_style() {
        let Some(display) = gdk::Display::default() else {
            return;
        };

        let provider = CssProvider::new();
        provider.load_from_data(
            ".layer-fixed {
                background: #f3f4f6;
            }

            .layer-panel {
                background: white;
                border: 1px solid #2f3437;
            }

            .layer-header {
                background: #e5e7eb;
                border-bottom: 1px solid #2f3437;
                padding: 6px 8px;
                font-weight: bold;
            }

            .layer-meta {
                color: #4b5563;
                font-weight: normal;
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
