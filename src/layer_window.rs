use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, gdk, glib};
use gtk4 as gtk;

#[derive(Clone)]
pub struct LayerWindow {
    title: Rc<RefCell<std::string::String>>,
    x: Rc<Cell<i32>>,
    y: Rc<Cell<i32>>,
    start_x_for_drag: Rc<Cell<f64>>,
    start_y_for_drag: Rc<Cell<f64>>,
    parent_fixed: Rc<RefCell<Option<gtk::Fixed>>>,
    scrolled_window: gtk::ScrolledWindow,
    frame: gtk::Box,
}

impl LayerWindow {
    pub fn new() -> Self {
        Self {
            title: Rc::new(RefCell::new("Layer 1".to_owned())),
            x: Rc::new(Cell::new(0)),
            y: Rc::new(Cell::new(0)),
            start_x_for_drag: Rc::new(Cell::new(0.0)),
            start_y_for_drag: Rc::new(Cell::new(0.0)),
            parent_fixed: Rc::new(RefCell::new(None)),
            scrolled_window: gtk::ScrolledWindow::new(),
            frame: gtk::Box::new(gtk::Orientation::Vertical, 0),
        }
    }

    fn set_draggable(&self, drag_handle: &impl IsA<gtk::Widget>) {
        const DRAG_START_THRESHOLD: f64 = 3.0;
        const DRAG_UPDATE_INTERVAL: Duration = Duration::from_millis(16);

        let drag = gtk4::GestureDrag::new();
        let drag_active = Rc::new(Cell::new(false));
        let pending_offset = Rc::new(Cell::new(None::<(f64, f64)>));
        let update_scheduled = Rc::new(Cell::new(false));

        {
            let x = self.x.clone();
            let y = self.y.clone();
            let start_x = self.start_x_for_drag.clone();
            let start_y = self.start_y_for_drag.clone();
            let drag_active = drag_active.clone();
            let pending_offset = pending_offset.clone();
            drag.connect_drag_begin(move |_, _, _| {
                start_x.set(x.get() as f64);
                start_y.set(y.get() as f64);
                drag_active.set(true);
                pending_offset.set(None);
            });
        }
        {
            let frame_cloned = self.frame.clone();
            let parent_fixed = self.parent_fixed.clone();
            let start_x = self.start_x_for_drag.clone();
            let start_y = self.start_y_for_drag.clone();
            let drag_active = drag_active.clone();
            let pending_offset = pending_offset.clone();
            let update_scheduled = update_scheduled.clone();

            drag.connect_drag_update(move |_, offset_x, offset_y| {
                if offset_x.hypot(offset_y) < DRAG_START_THRESHOLD {
                    pending_offset.set(None);
                    return;
                }

                pending_offset.set(Some((offset_x, offset_y)));
                if update_scheduled.get() {
                    return;
                }

                update_scheduled.set(true);
                let frame_cloned = frame_cloned.clone();
                let parent_fixed = parent_fixed.clone();
                let start_x = start_x.clone();
                let start_y = start_y.clone();
                let drag_active = drag_active.clone();
                let pending_offset = pending_offset.clone();
                let update_scheduled = update_scheduled.clone();

                glib::timeout_add_local(DRAG_UPDATE_INTERVAL, move || {
                    if !drag_active.get() {
                        pending_offset.set(None);
                        update_scheduled.set(false);
                        return glib::ControlFlow::Break;
                    }

                    let Some((offset_x, offset_y)) = pending_offset.take() else {
                        update_scheduled.set(false);
                        return glib::ControlFlow::Break;
                    };

                    let preview_x = (start_x.get() + offset_x).max(0.0).round();
                    let preview_y = (start_y.get() + offset_y).max(0.0).round();

                    if let Some(parent) = parent_fixed.borrow().as_ref() {
                        parent.move_(&frame_cloned, preview_x, preview_y);
                    }

                    glib::ControlFlow::Continue
                });
            });
        }
        {
            let frame_cloned = self.frame.clone();
            let parent_fixed = self.parent_fixed.clone();

            let x = self.x.clone();
            let y = self.y.clone();
            let start_x = self.start_x_for_drag.clone();
            let start_y = self.start_y_for_drag.clone();
            let drag_active = drag_active.clone();
            let pending_offset = pending_offset.clone();

            drag.connect_drag_end(move |_, offset_x, offset_y| {
                drag_active.set(false);
                pending_offset.set(None);

                if offset_x.hypot(offset_y) < DRAG_START_THRESHOLD {
                    return;
                }

                let tmp_x = start_x.get() as f64 + offset_x;
                let tmp_y = start_y.get() as f64 + offset_y;
                let final_x = tmp_x.max(0.0);
                let final_y = tmp_y.max(0.0);
                x.set(final_x.round() as i32);
                y.set(final_y.round() as i32);

                if let Some(parent) = parent_fixed.borrow().as_ref() {
                    parent.move_(&frame_cloned, x.get() as f64, y.get() as f64);
                }
            });
        }

        drag_handle.add_controller(drag);
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

    pub fn attach_to(&self, parent_fixed: &gtk::Fixed) {
        self.frame.set_halign(gtk::Align::Start);
        self.frame.set_valign(gtk::Align::Start);
        *self.parent_fixed.borrow_mut() = Some(parent_fixed.clone());
        parent_fixed.put(&self.frame, self.x.get() as f64, self.y.get() as f64);

        if let Some(header) = self.frame.first_child() {
            self.set_draggable(&header);
        }

        self.frame.show();
    }

    pub fn remove_from_parent(&self) {
        let Some(parent) = self.frame.parent() else {
            return;
        };

        if let Ok(parent_fixed) = parent.downcast::<gtk::Fixed>() {
            parent_fixed.remove(&self.frame);
        }
        *self.parent_fixed.borrow_mut() = None;
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
