use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;

#[derive(Clone)]
pub struct Canvas {
    fixed: gtk::Fixed,
}

struct ActiveDrag {
    target: gtk::Widget,
    start_x: f64,
    start_y: f64,
}

impl Canvas {
    pub fn new() -> Self {
        let fixed = gtk::Fixed::new();
        fixed.set_halign(gtk::Align::Fill);
        fixed.set_valign(gtk::Align::Fill);
        fixed.set_hexpand(true);
        fixed.set_vexpand(true);

        let canvas = Self { fixed };
        canvas.install_layer_drag();
        canvas
    }

    pub fn widget(&self) -> gtk::Widget {
        self.fixed.clone().upcast()
    }

    pub fn put_base_text_view(&self, text_view: &gtk::TextView) {
        self.fixed.put(text_view, 0.0, 0.0);
    }

    pub fn put_layer(&self, layer: &gtk::Widget, x: f64, y: f64) {
        self.fixed.put(layer, x, y);
    }

    pub fn move_layer(&self, layer: &gtk::Widget, x: f64, y: f64) {
        self.fixed.move_(layer, x.max(0.0), y.max(0.0));
    }

    pub fn remove_layer(&self, layer: &gtk::Widget) {
        let Some(parent) = layer.parent() else {
            return;
        };

        if parent == self.fixed.clone().upcast::<gtk::Widget>() {
            self.fixed.remove(layer);
        }
    }

    fn install_layer_drag(&self) {
        let active_drag = Rc::new(RefCell::new(None::<ActiveDrag>));
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);

        {
            let canvas = self.clone();
            let active_drag = active_drag.clone();
            drag.connect_drag_begin(move |gesture, start_x, start_y| {
                let Some(target) = canvas.draggable_layer_at(start_x, start_y) else {
                    active_drag.borrow_mut().take();
                    gesture.set_state(gtk::EventSequenceState::Denied);
                    return;
                };

                gesture.set_state(gtk::EventSequenceState::Claimed);
                let (start_x, start_y) = canvas.fixed.child_position(&target);
                active_drag.borrow_mut().replace(ActiveDrag {
                    target,
                    start_x,
                    start_y,
                });
            });
        }

        {
            let canvas = self.clone();
            let active_drag = active_drag.clone();
            drag.connect_drag_update(move |_, offset_x, offset_y| {
                let active_drag = active_drag.borrow();
                let Some(active_drag) = active_drag.as_ref() else {
                    return;
                };

                canvas.move_layer(
                    &active_drag.target,
                    active_drag.start_x + offset_x,
                    active_drag.start_y + offset_y,
                );
            });
        }

        {
            let canvas = self.clone();
            let active_drag = active_drag.clone();
            drag.connect_drag_end(move |_, offset_x, offset_y| {
                let Some(active) = active_drag.borrow_mut().take() else {
                    return;
                };

                canvas.move_layer(
                    &active.target,
                    active.start_x + offset_x,
                    active.start_y + offset_y,
                );
            });
        }

        self.fixed.add_controller(drag);
    }

    fn draggable_layer_at(&self, x: f64, y: f64) -> Option<gtk::Widget> {
        let fixed_widget: gtk::Widget = self.fixed.clone().upcast();
        let mut widget = self.fixed.pick(x, y, gtk::PickFlags::DEFAULT)?;
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
}
