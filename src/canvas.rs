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
    action: DragAction,
}

enum DragAction {
    Move { start_x: f64, start_y: f64 },
    Resize { start_width: i32, start_height: i32 },
}

impl Canvas {
    const MIN_LAYER_HEIGHT: i32 = 160;
    const MIN_LAYER_WIDTH: i32 = 240;

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

    pub fn get_base_text_view_position(&self, text_view: &gtk::TextView) -> (f64, f64) {
        self.fixed.child_position(text_view)
    }

    pub fn get_layer_position(&self, layer: &gtk::Widget) -> (f64, f64) {
        self.fixed.child_position(layer)
    }

    pub fn move_layer(&self, layer: &gtk::Widget, x: f64, y: f64) {
        self.fixed.move_(layer, x.max(0.0), y.max(0.0));
    }

    pub fn resize_layer(&self, layer: &gtk::Widget, width: i32, height: i32) {
        layer.set_size_request(
            width.max(Self::MIN_LAYER_WIDTH),
            height.max(Self::MIN_LAYER_HEIGHT),
        );
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
                let Some(interaction) = canvas.layer_interaction_at(start_x, start_y) else {
                    active_drag.borrow_mut().take();
                    gesture.set_state(gtk::EventSequenceState::Denied);
                    return;
                };

                gesture.set_state(gtk::EventSequenceState::Claimed);
                active_drag.borrow_mut().replace(match interaction {
                    LayerInteraction::Move(target) => {
                        let (start_x, start_y) = canvas.fixed.child_position(&target);
                        ActiveDrag {
                            target,
                            action: DragAction::Move { start_x, start_y },
                        }
                    }
                    LayerInteraction::Resize(target) => ActiveDrag {
                        action: DragAction::Resize {
                            start_width: target.width(),
                            start_height: target.height(),
                        },
                        target,
                    },
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

                canvas.update_active_drag(active_drag, offset_x, offset_y);
            });
        }

        {
            let canvas = self.clone();
            let active_drag = active_drag.clone();
            drag.connect_drag_end(move |_, offset_x, offset_y| {
                let Some(active) = active_drag.borrow_mut().take() else {
                    return;
                };

                canvas.update_active_drag(&active, offset_x, offset_y);
            });
        }

        self.fixed.add_controller(drag);
    }

    fn update_active_drag(&self, active_drag: &ActiveDrag, offset_x: f64, offset_y: f64) {
        match active_drag.action {
            DragAction::Move { start_x, start_y } => {
                self.move_layer(&active_drag.target, start_x + offset_x, start_y + offset_y)
            }
            DragAction::Resize {
                start_width,
                start_height,
            } => self.resize_layer(
                &active_drag.target,
                start_width + offset_x.round() as i32,
                start_height + offset_y.round() as i32,
            ),
        }
    }

    fn layer_interaction_at(&self, x: f64, y: f64) -> Option<LayerInteraction> {
        let fixed_widget: gtk::Widget = self.fixed.clone().upcast();
        let mut widget = self.fixed.pick(x, y, gtk::PickFlags::DEFAULT)?;
        let mut header_picked = false;
        let mut resize_picked = false;
        let mut close_picked = false;

        loop {
            if widget.has_css_class("layer-header") {
                header_picked = true;
            }
            if widget.has_css_class("layer-resize-handle") {
                resize_picked = true;
            }
            if widget.has_css_class("layer-close-button") {
                close_picked = true;
            }

            let parent = widget.parent()?;
            if parent == fixed_widget {
                if !widget.has_css_class("layer-panel") {
                    return None;
                }

                if close_picked {
                    return None;
                }
                if resize_picked {
                    return Some(LayerInteraction::Resize(widget));
                }
                if header_picked {
                    return Some(LayerInteraction::Move(widget));
                }

                return None;
            }

            widget = parent;
        }
    }
}

enum LayerInteraction {
    Move(gtk::Widget),
    Resize(gtk::Widget),
}
