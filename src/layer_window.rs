use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, gdk};
use gtk4 as gtk;

#[derive(Clone)]
pub struct LayerWindow {
    title: Rc<RefCell<std::string::String>>,
    x: Rc<Cell<i32>>,
    y: Rc<Cell<i32>>,
    scrolled_window: gtk::ScrolledWindow,
    frame: gtk::Box,
    parent_fixed: gtk::Fixed,
}

struct ActiveDrag {
    target: gtk::Widget,
    start_x: f64,
    start_y: f64,
}

impl LayerWindow {
    pub fn new(parent_fixed: &gtk::Fixed) -> Self {
        Self::install_fixed_drag(parent_fixed);

        Self {
            title: Rc::new(RefCell::new("Layer 1".to_owned())),
            x: Rc::new(Cell::new(0)),
            y: Rc::new(Cell::new(0)),
            scrolled_window: gtk::ScrolledWindow::new(),
            frame: gtk::Box::new(gtk::Orientation::Vertical, 0),
            parent_fixed: parent_fixed.clone(),
        }
    }

    fn install_fixed_drag(parent_fixed: &gtk::Fixed) {
        const FIXED_DRAG_INSTALLED_KEY: &str = "aa-editor-layer-fixed-drag-installed";

        // GTK object data is type-erased; the key is private to this module and only stores bool.
        if unsafe { parent_fixed.data::<bool>(FIXED_DRAG_INSTALLED_KEY) }.is_some() {
            return;
        }
        // GTK object data is type-erased; see the matching read above.
        unsafe {
            parent_fixed.set_data(FIXED_DRAG_INSTALLED_KEY, true);
        }

        let active_drag = Rc::new(RefCell::new(None::<ActiveDrag>));
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);

        {
            let parent_fixed = parent_fixed.clone();
            let active_drag = active_drag.clone();
            drag.connect_drag_begin(move |gesture, start_x, start_y| {
                let Some(target) = Self::draggable_layer_at(&parent_fixed, start_x, start_y) else {
                    active_drag.borrow_mut().take();
                    gesture.set_state(gtk::EventSequenceState::Denied);
                    return;
                };

                gesture.set_state(gtk::EventSequenceState::Claimed);
                let (start_x, start_y) = parent_fixed.child_position(&target);
                active_drag.borrow_mut().replace(ActiveDrag {
                    target,
                    start_x,
                    start_y,
                });
            });
        }

        {
            let parent_fixed = parent_fixed.clone();
            let active_drag = active_drag.clone();
            drag.connect_drag_update(move |_, offset_x, offset_y| {
                let active_drag = active_drag.borrow();
                let Some(active_drag) = active_drag.as_ref() else {
                    return;
                };

                parent_fixed.move_(
                    &active_drag.target,
                    (active_drag.start_x + offset_x).max(0.0),
                    (active_drag.start_y + offset_y).max(0.0),
                );
            });
        }

        {
            let parent_fixed = parent_fixed.clone();
            let active_drag = active_drag.clone();
            drag.connect_drag_end(move |_, offset_x, offset_y| {
                let Some(active) = active_drag.borrow_mut().take() else {
                    return;
                };

                parent_fixed.move_(
                    &active.target,
                    (active.start_x + offset_x).max(0.0),
                    (active.start_y + offset_y).max(0.0),
                );
            });
        }

        parent_fixed.add_controller(drag);
    }

    fn draggable_layer_at(parent_fixed: &gtk::Fixed, x: f64, y: f64) -> Option<gtk::Widget> {
        let fixed_widget: gtk::Widget = parent_fixed.clone().upcast();
        let mut widget = parent_fixed.pick(x, y, gtk::PickFlags::DEFAULT)?;
        let mut header_picked = false;

        loop {
            if widget.has_css_class("layer-header") {
                header_picked = true;
            }

            let parent = widget.parent()?;
            if parent == fixed_widget {
                return (header_picked && widget.has_css_class("layer-panel")).then_some(widget);
            }

            widget = parent;
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
        self.parent_fixed
            .put(&self.frame, self.x.get() as f64, self.y.get() as f64);

        self.frame.show();
    }

    pub fn remove_from_parent(&self) {
        let Some(parent) = self.frame.parent() else {
            return;
        };

        if let Ok(parent_fixed) = parent.downcast::<gtk::Fixed>() {
            parent_fixed.remove(&self.frame);
        }
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
