use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;

use gtk4 as gtk;

use gtk::gio;
use gtk::gio::prelude::{ActionMapExt, ApplicationExt};
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, ButtonsType, CssProvider, FileChooserAction, FileChooserNative,
    MessageDialog, MessageType, ResponseType, STYLE_PROVIDER_PRIORITY_APPLICATION, gdk,
};

use anyhow::{Context, Result};
use encoding_rs::SHIFT_JIS;

use crate::layer_window::LayerWindow;
use crate::project_file::ProjectFile;

const APP_TITLE: &str = "AA Editor";

#[derive(Debug, Default)]
struct ProjectState {
    current_path: Option<PathBuf>,
    project_file: ProjectFile,
    dirty: bool,
}

#[derive(Clone)]
struct LoadingControls {
    spinner: gtk::Spinner,
    editor_list: gtk::ListBox,
    add_button: gtk::Button,
    file_actions: Rc<RefCell<Vec<gio::SimpleAction>>>,
}

struct MainWindow {
    window: ApplicationWindow,
    v_box: gtk::Box,
    view_window: gtk::ScrolledWindow,
    editor_list: gtk::ListBox,
    mlt_tree_store: gtk::TreeStore,
    mlt_tree_view: gtk::TreeView,
    mlt_viewer_list: gtk::ListBox,
    layer_windows: Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
    editor_view_container: gtk::Fixed,
    overlay: gtk::Overlay,
    loading_spinner: gtk::Spinner,
    state: Rc<RefCell<ProjectState>>,
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
            editor_list: gtk::ListBox::new(),
            mlt_tree_store: gtk::TreeStore::new(&[
                String::static_type(),
                String::static_type(),
                bool::static_type(),
            ]),
            mlt_tree_view: gtk::TreeView::new(),
            mlt_viewer_list: gtk::ListBox::new(),
            layer_windows: Rc::new(RefCell::new(HashMap::new())),
            editor_view_container: gtk::Fixed::new(),
            overlay: gtk::Overlay::new(),
            loading_spinner: gtk::Spinner::new(),
            state: Rc::new(RefCell::new(ProjectState::default())),
        };

        result
    }

    fn install_text_style(&self) {
        let Some(display) = gdk::Display::default() else {
            return;
        };

        let provider = CssProvider::new();
        provider.load_from_data(
            ".aa-text-edit {
                border: 1px solid black;
                font-family: Monapo, 'MS PGothic', sans-serif;
                font-size: 16px;
                line-height: 18px;
            }

            .item-info {
                font-weight: bold;
                padding: 8px;
            }

            .loading-spinner {
                min-width: 48px;
                min-height: 48px;
            }

            .mlt-tree-path {
                padding: 8px;
            }",
        );

        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    fn project_title(state: &ProjectState) -> String {
        let file_name = state
            .current_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|file_name| file_name.to_str())
            .unwrap_or("Untitled");
        let dirty_marker = if state.dirty { " *" } else { "" };

        format!("{APP_TITLE} - {file_name}{dirty_marker}")
    }

    fn update_window_title(window: &gtk::ApplicationWindow, state: &Rc<RefCell<ProjectState>>) {
        let title = Self::project_title(&state.borrow());
        window.set_title(Some(&title));
    }

    fn mark_dirty(window: &gtk::ApplicationWindow, state: &Rc<RefCell<ProjectState>>) {
        let mut state_ref = state.borrow_mut();
        if state_ref.dirty {
            return;
        }

        state_ref.dirty = true;
        drop(state_ref);
        Self::update_window_title(window, state);
    }

    fn mark_clean(
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        path: Option<PathBuf>,
    ) {
        {
            let mut state_ref = state.borrow_mut();
            state_ref.current_path = path;
            state_ref.dirty = false;
        }

        Self::update_window_title(window, state);
    }

    fn text_stats(text: &str) -> (usize, usize, usize) {
        let line_count = text.lines().count().max(1);
        let max_width = text
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        let char_count = text.chars().count();

        (max_width, line_count, char_count)
    }

    fn update_item_info_label(
        item_number: usize,
        buffer: &gtk::TextBuffer,
        info_label: &gtk::Label,
    ) {
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, true);
        let (max_width, line_count, char_count) = Self::text_stats(&text);

        info_label.set_text(&format!(
            "No. {item_number}\n横幅: {max_width}\n縦幅: {line_count}\n文字数: {char_count}\n行数: {line_count}"
        ));
    }

    fn item_number_from_info_label(info_label: &gtk::Label) -> usize {
        info_label
            .text()
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("No. "))
            .and_then(|number| number.parse::<usize>().ok())
            .unwrap_or(0)
    }

    fn create_editor_row(
        text: &str,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: Option<&Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>>,
        editor_view_container: Option<&gtk::Fixed>,
        read_only: bool,
    ) -> gtk::ListBoxRow {
        let editor_row = gtk::ListBoxRow::new();
        editor_row.set_margin_top(6);
        editor_row.set_margin_bottom(6);
        editor_row.set_margin_start(8);
        editor_row.set_margin_end(8);
        editor_row.set_vexpand(true);
        editor_row.set_activatable(false);
        editor_row.set_selectable(false);

        // whole pane
        let row_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        row_paned.set_hexpand(true);
        row_paned.set_vexpand(false);

        let left_pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left_pane.set_size_request(120, -1);
        left_pane.set_valign(gtk::Align::Start);

        // for some info at left pane
        let item_info = gtk::Label::new(None);
        item_info.add_css_class("item-info");
        item_info.set_xalign(0.0);
        item_info.set_valign(gtk::Align::Start);

        let text_view = gtk::TextView::new();
        text_view.add_css_class("aa-text-edit");
        text_view.set_left_margin(8);
        text_view.set_right_margin(8);
        text_view.set_top_margin(8);
        text_view.set_bottom_margin(8);
        text_view.set_wrap_mode(gtk::WrapMode::None);
        text_view.set_accepts_tab(true);
        // text_view.set_monospace(true);
        text_view.set_editable(!read_only);
        text_view.set_cursor_visible(!read_only);
        text_view.set_hexpand(true);
        text_view.set_vexpand(true);
        text_view.set_size_request(1024, 768);
        text_view.buffer().set_text(text);
        Self::update_item_info_label(0, &text_view.buffer(), &item_info);

        if !read_only {
            let window_for_change = window.clone();
            let state_for_change = state.clone();
            let item_info_for_change = item_info.clone();
            text_view.buffer().connect_changed(move |buffer| {
                let item_number = Self::item_number_from_info_label(&item_info_for_change);
                Self::update_item_info_label(item_number, buffer, &item_info_for_change);
                Self::mark_dirty(&window_for_change, &state_for_change);
            });
        }

        left_pane.append(&item_info);

        let editor_overlay = gtk::Overlay::new();
        editor_overlay.set_hexpand(true);
        editor_overlay.set_vexpand(true);

        if let (Some(layer_windows), Some(editor_view_container)) =
            (layer_windows, editor_view_container)
        {
            let add_layer_button = gtk::Button::with_label("Add Layer");
            add_layer_button.set_valign(gtk::Align::Start);

            editor_view_container.put(&text_view, 0.0, 0.0);
            editor_overlay.set_child(Some(editor_view_container));

            let item_info_for_layer = item_info.clone();
            let layer_windows = layer_windows.clone();
            let editor_view_container = editor_view_container.clone();
            // let layer_host = layer_host.clone();
            add_layer_button.connect_clicked(move |_| {
                let item_number = Self::item_number_from_info_label(&item_info_for_layer) as u64;
                if item_number == 0 {
                    return;
                }

                let layer_number = layer_windows
                    .borrow()
                    .get(&item_number)
                    .map(|layers| layers.len() + 1)
                    .unwrap_or(1);

                let layer_window = LayerWindow::new(&editor_view_container);
                let title = format!("Item {item_number} Layer {layer_number}");
                layer_window.init(title, 640, 480);
                layer_window.attach_to();

                layer_windows
                    .borrow_mut()
                    .entry(item_number)
                    .or_default()
                    .push(layer_window);
            });

            left_pane.append(&add_layer_button);
        }

        let editor_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        editor_paned.set_hexpand(true);
        editor_paned.set_vexpand(true);
        editor_paned.set_start_child(Some(&editor_overlay));
        editor_paned.set_resize_start_child(true);
        editor_paned.set_shrink_start_child(false);

        if !read_only {
            let controls_pane = gtk::Box::new(gtk::Orientation::Vertical, 8);
            controls_pane.set_size_request(96, -1);
            controls_pane.set_valign(gtk::Align::Start);

            let close_button = gtk::Button::with_label("Close");
            close_button.set_valign(gtk::Align::Start);

            let row_for_delete = editor_row.clone();
            let window_for_delete = window.clone();
            let state_for_delete = state.clone();
            let layer_windows_for_delete = layer_windows.cloned();
            close_button.connect_clicked(move |_| {
                if let Some(layer_windows) = &layer_windows_for_delete {
                    MainWindow::confirm_remove_editor_row(
                        &row_for_delete,
                        &window_for_delete,
                        &state_for_delete,
                        layer_windows,
                    );
                }
            });
            controls_pane.append(&close_button);

            editor_paned.set_end_child(Some(&controls_pane));
            editor_paned.set_resize_end_child(false);
            editor_paned.set_shrink_end_child(false);
            editor_paned.set_position(1024);
        }

        row_paned.set_start_child(Some(&left_pane));
        row_paned.set_resize_start_child(false);
        row_paned.set_shrink_start_child(false);
        row_paned.set_end_child(Some(&editor_paned));
        row_paned.set_resize_end_child(true);
        row_paned.set_shrink_end_child(false);
        row_paned.set_position(136);

        editor_row.set_child(Some(&row_paned));
        editor_row
    }

    fn append_editor(
        editor_list: &gtk::ListBox,
        text: &str,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        editor_view_container: &gtk::Fixed,
    ) {
        editor_list.append(&Self::create_editor_row(
            text,
            window,
            state,
            Some(layer_windows),
            Some(editor_view_container),
            false,
        ));
        Self::renumber_editors(editor_list);
    }

    fn append_read_only_viewer_item(
        viewer_list: &gtk::ListBox,
        text: &str,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
    ) {
        viewer_list.append(&Self::create_editor_row(
            text, window, state, None, None, true,
        ));
        Self::renumber_editors(viewer_list);
    }

    fn clear_editors(editor_list: &gtk::ListBox) {
        while let Some(child) = editor_list.first_child() {
            editor_list.remove(&child);
        }
    }

    fn clear_layer_windows(layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>) {
        let mut layer_windows = layer_windows.borrow_mut();
        for layers in layer_windows.values() {
            for layer in layers {
                layer.remove_from_parent();
            }
        }
        layer_windows.clear();
    }

    fn set_loading(spinner: &gtk::Spinner, loading: bool) {
        spinner.set_visible(loading);
        if loading {
            spinner.start();
        } else {
            spinner.stop();
        }
    }

    fn set_buttons_enabled(widget: &gtk::Widget, enabled: bool) {
        if let Ok(button) = widget.clone().downcast::<gtk::Button>() {
            button.set_sensitive(enabled);
        }

        let mut child = widget.first_child();
        while let Some(child_widget) = child {
            child = child_widget.next_sibling();
            Self::set_buttons_enabled(&child_widget, enabled);
        }
    }

    fn set_close_buttons_enabled(editor_list: &gtk::ListBox, enabled: bool) {
        let mut child = editor_list.first_child();

        while let Some(row_widget) = child {
            child = row_widget.next_sibling();

            let Ok(row) = row_widget.clone().downcast::<gtk::ListBoxRow>() else {
                continue;
            };
            if let Some(content) = row.child() {
                Self::set_buttons_enabled(&content, enabled);
            }
        }
    }

    fn set_loading_state(controls: &LoadingControls, loading: bool) {
        let enabled = !loading;

        Self::set_loading(&controls.spinner, loading);
        controls.add_button.set_sensitive(enabled);
        Self::set_close_buttons_enabled(&controls.editor_list, enabled);
        for action in controls.file_actions.borrow().iter() {
            action.set_enabled(enabled);
        }
    }

    fn show_error_dialog(window: &gtk::ApplicationWindow, message: &str) {
        let dialog = MessageDialog::builder()
            .transient_for(window)
            .modal(true)
            .message_type(MessageType::Error)
            .buttons(ButtonsType::Ok)
            .text("ファイルの読み込みに失敗しました")
            .secondary_text(message)
            .build();

        dialog.run_async(|dialog, _| {
            dialog.close();
        });
    }

    fn load_project_from_path(path: &Path) -> Result<ProjectFile> {
        ProjectFile::read_from_path(path)
    }

    fn apply_project_file(
        editor_list: &gtk::ListBox,
        mlt_tree_store: &gtk::TreeStore,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        editor_view_container: &gtk::Fixed,
        path: PathBuf,
        project_file: ProjectFile,
    ) {
        Self::clear_editors(editor_list);
        Self::clear_layer_windows(layer_windows);
        let texts = project_file.to_texts();

        if texts.is_empty() {
            Self::append_editor(
                editor_list,
                "",
                window,
                state,
                layer_windows,
                editor_view_container,
            );
        } else {
            for text in texts {
                Self::append_editor(
                    editor_list,
                    &text,
                    window,
                    state,
                    layer_windows,
                    editor_view_container,
                );
            }
        }

        state.borrow_mut().project_file = project_file;
        Self::refresh_mlt_collection_tree(mlt_tree_store, state);
        Self::mark_clean(window, state, Some(path));
    }

    fn apply_mlt_texts(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        editor_view_container: &gtk::Fixed,
        texts: Vec<String>,
    ) {
        for text in texts {
            Self::append_editor(
                editor_list,
                &text,
                window,
                state,
                layer_windows,
                editor_view_container,
            );
        }
        Self::mark_dirty(window, state);
    }

    fn renumber_editors(editor_list: &gtk::ListBox) {
        let mut index = 1;
        let mut child = editor_list.first_child();

        while let Some(row_widget) = child {
            child = row_widget.next_sibling();

            let Ok(row) = row_widget.downcast::<gtk::ListBoxRow>() else {
                continue;
            };
            let Some(content) = row.child() else {
                continue;
            };
            let Some(info_label) = Self::info_label_from_row_content(content.clone()) else {
                continue;
            };
            let Some(text_view) = Self::text_view_from_row_content(content) else {
                continue;
            };

            Self::update_item_info_label(index, &text_view.buffer(), &info_label);
            index += 1;
        }
    }

    fn item_number_from_row(row: &gtk::ListBoxRow) -> Option<u64> {
        let info_label = Self::info_label_from_row_content(row.child()?)?;

        Some(Self::item_number_from_info_label(&info_label) as u64)
    }

    fn remove_layer_windows_for_item(
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        removed_item_number: u64,
    ) {
        let mut layer_windows = layer_windows.borrow_mut();
        if let Some(layers) = layer_windows.remove(&removed_item_number) {
            for layer in layers {
                layer.remove_from_parent();
            }
        }

        let mut keys_to_shift = layer_windows
            .keys()
            .copied()
            .filter(|key| *key > removed_item_number)
            .collect::<Vec<_>>();
        keys_to_shift.sort_unstable();

        for key in keys_to_shift {
            if let Some(layers) = layer_windows.remove(&key) {
                layer_windows.insert(key - 1, layers);
            }
        }
    }

    fn confirm_remove_editor_row(
        row: &gtk::ListBoxRow,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
    ) {
        let dialog = MessageDialog::builder()
            .transient_for(window)
            .modal(true)
            .message_type(MessageType::Warning)
            .buttons(ButtonsType::OkCancel)
            .text("このItemを削除しますか？")
            .secondary_text("OKを押すと、このItemは削除されます。")
            .build();

        let row = row.clone();
        let window = window.clone();
        let state = state.clone();
        let layer_windows = layer_windows.clone();
        dialog.run_async(move |dialog, response| {
            if response == ResponseType::Ok {
                if let Some(parent) = row
                    .parent()
                    .and_then(|parent| parent.downcast::<gtk::ListBox>().ok())
                {
                    if let Some(item_number) = MainWindow::item_number_from_row(&row) {
                        MainWindow::remove_layer_windows_for_item(&layer_windows, item_number);
                    }
                    parent.remove(&row);
                    MainWindow::renumber_editors(&parent);
                    MainWindow::mark_dirty(&window, &state);
                }
            }

            dialog.close();
        });
    }

    fn collect_editor_texts(editor_list: &gtk::ListBox) -> Vec<String> {
        let mut texts = Vec::new();
        let mut child = editor_list.first_child();

        while let Some(row_widget) = child {
            child = row_widget.next_sibling();

            let Ok(row) = row_widget.downcast::<gtk::ListBoxRow>() else {
                continue;
            };
            let Some(content_widget) = row.child() else {
                continue;
            };
            let Some(text_view) = Self::text_view_from_row_content(content_widget) else {
                continue;
            };

            let buffer = text_view.buffer();
            let start = buffer.start_iter();
            let end = buffer.end_iter();
            texts.push(buffer.text(&start, &end, true).to_string());
        }

        texts
    }

    fn info_label_from_row_content(widget: gtk::Widget) -> Option<gtk::Label> {
        if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
            if label.has_css_class("item-info") {
                return Some(label);
            }
        }

        let mut child = widget.first_child();
        while let Some(child_widget) = child {
            child = child_widget.next_sibling();
            if let Some(label) = Self::info_label_from_row_content(child_widget) {
                return Some(label);
            }
        }

        None
    }

    fn text_view_from_row_content(widget: gtk::Widget) -> Option<gtk::TextView> {
        if let Ok(text_view) = widget.clone().downcast::<gtk::TextView>() {
            return Some(text_view);
        }

        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Some(text_view) = Self::text_view_from_row_content(widget) {
                return Some(text_view);
            }
        }

        None
    }
    fn load_project_from_path_async(
        editor_list: &gtk::ListBox,
        mlt_tree_store: &gtk::TreeStore,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        editor_view_container: &gtk::Fixed,
        loading_controls: &LoadingControls,
        path: PathBuf,
    ) {
        Self::set_loading_state(loading_controls, true);

        let editor_list = editor_list.clone();
        let mlt_tree_store = mlt_tree_store.clone();
        let window = window.clone();
        let state = state.clone();
        let layer_windows = layer_windows.clone();
        let editor_view_container = editor_view_container.clone();
        let loading_controls = loading_controls.clone();
        glib::spawn_future_local(async move {
            let (sender, receiver) = mpsc::channel();
            let path_for_worker = path.clone();

            std::thread::spawn(move || {
                let result = MainWindow::load_project_from_path(&path_for_worker)
                    .map_err(|error| error.to_string());
                let _ = sender.send(result);
            });

            glib::idle_add_local(move || match receiver.try_recv() {
                Ok(Ok(project_file)) => {
                    MainWindow::apply_project_file(
                        &editor_list,
                        &mlt_tree_store,
                        &window,
                        &state,
                        &layer_windows,
                        &editor_view_container,
                        path.clone(),
                        project_file,
                    );
                    MainWindow::set_loading_state(&loading_controls, false);
                    glib::ControlFlow::Break
                }
                Ok(Err(error)) => {
                    MainWindow::set_loading_state(&loading_controls, false);
                    MainWindow::show_error_dialog(&window, &error);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let error = "failed to receive project load result";
                    MainWindow::set_loading_state(&loading_controls, false);
                    MainWindow::show_error_dialog(&window, error);
                    glib::ControlFlow::Break
                }
            });
        });
    }

    fn save_project_to_path(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        path: &Path,
    ) -> Result<()> {
        let mut project_file = state.borrow().project_file.clone();
        project_file.set_texts(Self::collect_editor_texts(editor_list));
        project_file.write_to_path(path)?;
        state.borrow_mut().project_file = project_file;
        Self::mark_clean(window, state, Some(path.to_path_buf()));
        Ok(())
    }

    fn save_project(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
    ) {
        let path = state.borrow().current_path.clone();
        if let Some(path) = path {
            if let Err(e) = Self::save_project_to_path(editor_list, window, state, &path) {
                eprintln!("{e}");
            }
        } else {
            Self::show_save_as_dialog(editor_list, window, state);
        }
    }

    fn path_with_json_extension(path: PathBuf) -> PathBuf {
        if path.extension().is_some() {
            path
        } else {
            path.with_extension("aa_editor_proj")
        }
    }

    fn path_with_mlt_extension(path: PathBuf) -> PathBuf {
        if path.extension().is_some() {
            path
        } else {
            path.with_extension("mlt")
        }
    }

    fn show_open_dialog(
        editor_list: &gtk::ListBox,
        mlt_tree_store: &gtk::TreeStore,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        editor_view_container: &gtk::Fixed,
        loading_controls: &LoadingControls,
    ) {
        let dialog = FileChooserNative::new(
            Some("プロジェクトを開く"),
            Some(window),
            FileChooserAction::Open,
            Some("Open"),
            Some("Cancel"),
        );

        let editor_list = editor_list.clone();
        let mlt_tree_store = mlt_tree_store.clone();
        let window = window.clone();
        let state = state.clone();
        let layer_windows = layer_windows.clone();
        let editor_view_container_cloned = editor_view_container.clone();
        let loading_controls = loading_controls.clone();
        dialog.run_async(move |dialog, response| {
            if response == ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    MainWindow::load_project_from_path_async(
                        &editor_list,
                        &mlt_tree_store,
                        &window,
                        &state,
                        &layer_windows,
                        &editor_view_container_cloned,
                        &loading_controls,
                        path,
                    );
                }
            }

            dialog.destroy();
        });
    }

    fn show_save_as_dialog(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
    ) {
        let dialog = FileChooserNative::new(
            Some("名前を付けて保存"),
            Some(window),
            FileChooserAction::Save,
            Some("Save"),
            Some("Cancel"),
        );

        let editor_list = editor_list.clone();
        let window = window.clone();
        let state = state.clone();
        dialog.run_async(move |dialog, response| {
            if response == ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    let path = MainWindow::path_with_json_extension(path);
                    if let Err(e) =
                        MainWindow::save_project_to_path(&editor_list, &window, &state, &path)
                    {
                        eprintln!("{e}");
                    }
                }
            }

            dialog.destroy();
        });
    }

    fn show_export_mlt_dialog(editor_list: &gtk::ListBox, window: &gtk::ApplicationWindow) {
        let dialog = FileChooserNative::new(
            Some("MLT(UTF-8)で出力"),
            Some(window),
            FileChooserAction::Save,
            Some("Export"),
            Some("Cancel"),
        );

        let editor_list = editor_list.clone();
        dialog.run_async(move |dialog, response| {
            if response == ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    let path = MainWindow::path_with_mlt_extension(path);
                    let mlt_text = MainWindow::collect_editor_texts(&editor_list).join("[SPLIT]");

                    if let Err(e) = fs::write(&path, mlt_text) {
                        eprintln!("failed to write mlt file: {}: {e}", path.display());
                    }
                }
            }

            dialog.destroy();
        });
    }

    fn decode_mlt_text(bytes: Vec<u8>) -> String {
        match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                let bytes = error.into_bytes();
                let (text, _, _) = SHIFT_JIS.decode(&bytes);
                text.into_owned()
            }
        }
    }

    fn load_mlt_texts_from_path(path: &Path) -> Result<Vec<String>> {
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read mlt file: {}", path.display()))?;
        let text = Self::decode_mlt_text(bytes);

        Ok(text.split("[SPLIT]").map(|text| text.to_string()).collect())
    }

    fn apply_mlt_viewer_texts(
        viewer_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        texts: Vec<String>,
    ) {
        Self::clear_editors(viewer_list);

        if texts.is_empty() {
            Self::append_read_only_viewer_item(viewer_list, "", window, state);
        } else {
            for text in texts {
                Self::append_read_only_viewer_item(viewer_list, &text, window, state);
            }
        }
    }

    fn load_mlt_viewer_from_path(
        viewer_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        path: PathBuf,
    ) {
        match Self::load_mlt_texts_from_path(&path) {
            Ok(texts) => Self::apply_mlt_viewer_texts(viewer_list, window, state, texts),
            Err(error) => Self::show_error_dialog(window, &error.to_string()),
        }
    }

    fn resolve_project_path(path: &str) -> PathBuf {
        if path == "~" {
            if let Some(home) = env::var_os("HOME") {
                return PathBuf::from(home);
            }
        }

        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = env::var_os("HOME") {
                return PathBuf::from(home).join(rest);
            }
        }

        PathBuf::from(path)
    }

    fn mlt_collection_directory_path(state: &Rc<RefCell<ProjectState>>) -> PathBuf {
        Self::resolve_project_path(&state.borrow().project_file.mlt_collection_directory_path)
    }

    fn is_mlt_file(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mlt"))
    }

    fn append_mlt_tree_path(
        store: &gtk::TreeStore,
        parent: Option<&gtk::TreeIter>,
        path: &Path,
    ) -> Result<()> {
        let display_name = path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map(|file_name| file_name.to_string())
            .unwrap_or_else(|| path.display().to_string());
        let is_file = path.is_file();
        let full_path = if is_file {
            path.display().to_string()
        } else {
            String::new()
        };

        let iter = store.insert_with_values(
            parent,
            None,
            &[(0, &display_name), (1, &full_path), (2, &is_file)],
        );

        if !path.is_dir() {
            return Ok(());
        }

        let mut entries = fs::read_dir(path)
            .with_context(|| format!("failed to read directory: {}", path.display()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("failed to read directory entry: {}", path.display()))?;

        entries.retain(|entry| {
            let path = entry.path();
            path.is_dir() || Self::is_mlt_file(&path)
        });
        entries.sort_by_key(|entry| {
            let path = entry.path();
            (
                path.is_file(),
                entry.file_name().to_string_lossy().to_lowercase(),
            )
        });

        for entry in entries {
            Self::append_mlt_tree_path(store, Some(&iter), &entry.path())?;
        }

        Ok(())
    }

    fn refresh_mlt_collection_tree(store: &gtk::TreeStore, state: &Rc<RefCell<ProjectState>>) {
        store.clear();

        let root_path = Self::mlt_collection_directory_path(state);
        if let Err(error) = Self::append_mlt_tree_path(store, None, &root_path) {
            let message = format!("{} ({error})", root_path.display());
            store.insert_with_values(
                None,
                None,
                &[(0, &message), (1, &String::new()), (2, &false)],
            );
        }
    }

    fn install_mlt_tree_view(
        tree_view: &gtk::TreeView,
        store: &gtk::TreeStore,
        viewer_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
    ) {
        tree_view.set_model(Some(store));
        tree_view.set_headers_visible(false);
        tree_view.set_enable_tree_lines(true);
        tree_view.set_show_expanders(true);

        let renderer = gtk::CellRendererText::new();
        let column = gtk::TreeViewColumn::new();
        column.pack_start(&renderer, true);
        column.add_attribute(&renderer, "text", 0);
        tree_view.append_column(&column);

        let viewer_list = viewer_list.clone();
        let window = window.clone();
        let state = state.clone();
        tree_view.connect_row_activated(move |tree_view, tree_path, _| {
            let Some(model) = tree_view.model() else {
                return;
            };
            let Some(iter) = model.iter(tree_path) else {
                return;
            };

            let is_file = model.get::<bool>(&iter, 2);
            if !is_file {
                if tree_view.row_expanded(tree_path) {
                    tree_view.collapse_row(tree_path);
                } else {
                    tree_view.expand_row(tree_path, false);
                }
                return;
            }

            let path = PathBuf::from(model.get::<String>(&iter, 1));
            MainWindow::load_mlt_viewer_from_path(&viewer_list, &window, &state, path);
        });

        tree_view.expand_all();
    }

    fn load_mlt_from_path_async(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        editor_view_container: &gtk::Fixed,
        loading_controls: &LoadingControls,
        path: PathBuf,
    ) {
        Self::set_loading_state(loading_controls, true);

        let editor_list = editor_list.clone();
        let window = window.clone();
        let state = state.clone();
        let layer_windows = layer_windows.clone();
        let editor_view_container_cloned = editor_view_container.clone();
        let loading_controls = loading_controls.clone();
        glib::spawn_future_local(async move {
            let (sender, receiver) = mpsc::channel();

            std::thread::spawn(move || {
                let result =
                    MainWindow::load_mlt_texts_from_path(&path).map_err(|error| error.to_string());
                let _ = sender.send(result);
            });

            glib::idle_add_local(move || match receiver.try_recv() {
                Ok(Ok(texts)) => {
                    MainWindow::apply_mlt_texts(
                        &editor_list,
                        &window,
                        &state,
                        &layer_windows,
                        &editor_view_container_cloned,
                        texts,
                    );
                    MainWindow::set_loading_state(&loading_controls, false);
                    glib::ControlFlow::Break
                }
                Ok(Err(error)) => {
                    MainWindow::set_loading_state(&loading_controls, false);
                    MainWindow::show_error_dialog(&window, &error);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let error = "failed to receive mlt load result";
                    MainWindow::set_loading_state(&loading_controls, false);
                    MainWindow::show_error_dialog(&window, error);
                    glib::ControlFlow::Break
                }
            });
        });
    }

    fn show_import_mlt_dialog(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        editor_view_container: &gtk::Fixed,
        loading_controls: &LoadingControls,
    ) {
        let dialog = FileChooserNative::new(
            Some("MLTを読み込み"),
            Some(window),
            FileChooserAction::Open,
            Some("Open"),
            Some("Cancel"),
        );

        let editor_list = editor_list.clone();
        let window = window.clone();
        let state = state.clone();
        let layer_windows = layer_windows.clone();
        let editor_view_container_cloned = editor_view_container.clone();
        let loading_controls = loading_controls.clone();
        dialog.run_async(move |dialog, response| {
            if response == ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    MainWindow::load_mlt_from_path_async(
                        &editor_list,
                        &window,
                        &state,
                        &layer_windows,
                        &editor_view_container_cloned,
                        &loading_controls,
                        path,
                    );
                }
            }

            dialog.destroy();
        });
    }

    fn install_file_menu(
        app: &Application,
        editor_list: &gtk::ListBox,
        mlt_tree_store: &gtk::TreeStore,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        layer_windows: &Rc<RefCell<HashMap<u64, Vec<LayerWindow>>>>,
        editor_view_container: &gtk::Fixed,
        spinner: &gtk::Spinner,
        add_button: &gtk::Button,
    ) {
        let file_actions = Rc::new(RefCell::new(Vec::new()));
        let loading_controls = LoadingControls {
            spinner: spinner.clone(),
            editor_list: editor_list.clone(),
            add_button: add_button.clone(),
            file_actions: file_actions.clone(),
        };

        let file_menu = gio::Menu::new();
        file_menu.append(Some("プロジェクトを開く"), Some("app.open-project"));
        file_menu.append(Some("プロジェクトを上書きで保存"), Some("app.save-project"));
        file_menu.append(
            Some("プロジェクトを名前を付けて保存"),
            Some("app.save-project-as"),
        );
        file_menu.append(Some("MLTを読み込み"), Some("app.import-mlt"));
        file_menu.append(Some("MLTで出力"), Some("app.export-mlt"));
        file_menu.append(Some("終了"), Some("app.quit"));

        let menu_bar = gio::Menu::new();
        menu_bar.append_submenu(Some("ファイル"), &file_menu);
        app.set_menubar(Some(&menu_bar));

        let open_action = gio::SimpleAction::new("open-project", None);
        let editor_list_clone = editor_list.clone();
        let mlt_tree_store_clone = mlt_tree_store.clone();
        let window_clone = window.clone();
        let state_clone = state.clone();
        let layer_windows_clone = layer_windows.clone();
        let editor_view_container_cloned = editor_view_container.clone();
        let loading_controls_clone = loading_controls.clone();
        open_action.connect_activate(move |_, _| {
            MainWindow::show_open_dialog(
                &editor_list_clone,
                &mlt_tree_store_clone,
                &window_clone,
                &state_clone,
                &layer_windows_clone,
                &editor_view_container_cloned,
                &loading_controls_clone,
            );
        });
        app.add_action(&open_action);
        file_actions.borrow_mut().push(open_action.clone());

        let save_action = gio::SimpleAction::new("save-project", None);
        let editor_list_clone = editor_list.clone();
        let window_clone = window.clone();
        let state_clone = state.clone();
        save_action.connect_activate(move |_, _| {
            MainWindow::save_project(&editor_list_clone, &window_clone, &state_clone);
        });
        app.add_action(&save_action);
        file_actions.borrow_mut().push(save_action.clone());
        app.set_accels_for_action("app.save-project", &["<Control>s"]);

        let save_as_action = gio::SimpleAction::new("save-project-as", None);
        let editor_list_clone = editor_list.clone();
        let window_clone = window.clone();
        let state_clone = state.clone();
        save_as_action.connect_activate(move |_, _| {
            MainWindow::show_save_as_dialog(&editor_list_clone, &window_clone, &state_clone);
        });
        app.add_action(&save_as_action);
        file_actions.borrow_mut().push(save_as_action.clone());

        let import_mlt_action = gio::SimpleAction::new("import-mlt", None);
        let editor_list_clone = editor_list.clone();
        let window_clone = window.clone();
        let state_clone = state.clone();
        let layer_windows_clone = layer_windows.clone();
        let editor_view_container_cloned = editor_view_container.clone();
        let loading_controls_clone = loading_controls.clone();
        import_mlt_action.connect_activate(move |_, _| {
            MainWindow::show_import_mlt_dialog(
                &editor_list_clone,
                &window_clone,
                &state_clone,
                &layer_windows_clone,
                &editor_view_container_cloned,
                &loading_controls_clone,
            );
        });
        app.add_action(&import_mlt_action);
        file_actions.borrow_mut().push(import_mlt_action.clone());

        let export_mlt_action = gio::SimpleAction::new("export-mlt", None);
        let editor_list_clone = editor_list.clone();
        let window_clone = window.clone();
        export_mlt_action.connect_activate(move |_, _| {
            MainWindow::show_export_mlt_dialog(&editor_list_clone, &window_clone);
        });
        app.add_action(&export_mlt_action);
        file_actions.borrow_mut().push(export_mlt_action.clone());

        let quit_action = gio::SimpleAction::new("quit", None);
        let app_clone = app.clone();
        quit_action.connect_activate(move |_, _| {
            app_clone.quit();
        });
        app.add_action(&quit_action);
    }

    fn init(&self, app: &Application, width: i32, height: i32) -> Result<()> {
        self.window.set_default_size(width, height);
        self.window.set_show_menubar(true);

        // root box
        self.v_box.set_halign(gtk::Align::Fill);
        self.v_box.set_valign(gtk::Align::Fill);
        self.v_box.set_hexpand(true);
        self.v_box.set_vexpand(true);

        // main view. main view have 3 pane
        let main_view_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
        main_view_box.set_halign(gtk::Align::Fill);
        main_view_box.set_valign(gtk::Align::Fill);
        main_view_box.set_hexpand(true);
        main_view_box.set_vexpand(true);

        // editor container
        self.editor_view_container.set_halign(gtk::Align::Fill);
        self.editor_view_container.set_valign(gtk::Align::Fill);
        self.editor_view_container.set_hexpand(true);
        self.editor_view_container.set_vexpand(true);

        self.install_text_style();

        self.editor_list
            .set_selection_mode(gtk::SelectionMode::None);
        self.editor_list.set_hexpand(true);
        self.editor_list.set_vexpand(true);
        Self::append_editor(
            &self.editor_list,
            "",
            &self.window,
            &self.state,
            &self.layer_windows,
            &self.editor_view_container,
        );
        Self::mark_clean(&self.window, &self.state, None);

        // add button on overlay
        let add_button = gtk::Button::with_label("Add");
        add_button.set_halign(gtk::Align::End);
        add_button.set_valign(gtk::Align::End);
        add_button.set_margin_end(16);
        add_button.set_margin_bottom(16);

        let editor_list = self.editor_list.clone();
        let window = self.window.clone();
        let state = self.state.clone();
        let layer_windows = self.layer_windows.clone();
        let editor_view_container_cloned = self.editor_view_container.clone();
        add_button.connect_clicked(move |_| {
            MainWindow::append_editor(
                &editor_list,
                "",
                &window,
                &state,
                &layer_windows,
                &editor_view_container_cloned,
            );
            MainWindow::mark_dirty(&window, &state);
        });

        self.view_window.set_child(Some(&self.editor_list));
        self.view_window
            .set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        self.view_window.set_hexpand(true);
        self.view_window.set_vexpand(true);

        let editor_overlay = gtk::Overlay::new();
        editor_overlay.set_halign(gtk::Align::Fill);
        editor_overlay.set_valign(gtk::Align::Fill);
        editor_overlay.set_hexpand(true);
        editor_overlay.set_vexpand(true);
        editor_overlay.set_child(Some(&self.view_window));
        editor_overlay.add_overlay(&add_button);
        main_view_box.append(&editor_overlay);

        let mlt_tree_scroll = gtk::ScrolledWindow::new();
        mlt_tree_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        mlt_tree_scroll.set_min_content_width(280);
        mlt_tree_scroll.set_child(Some(&self.mlt_tree_view));

        self.mlt_viewer_list
            .set_selection_mode(gtk::SelectionMode::None);
        self.mlt_viewer_list.set_hexpand(true);
        self.mlt_viewer_list.set_vexpand(true);
        let mlt_viewer_scroll = gtk::ScrolledWindow::new();
        mlt_viewer_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        mlt_viewer_scroll.set_child(Some(&self.mlt_viewer_list));

        let mlt_viewer_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        mlt_viewer_paned.set_start_child(Some(&mlt_tree_scroll));
        mlt_viewer_paned.set_resize_start_child(false);
        mlt_viewer_paned.set_shrink_start_child(false);
        mlt_viewer_paned.set_end_child(Some(&mlt_viewer_scroll));
        mlt_viewer_paned.set_resize_end_child(true);
        mlt_viewer_paned.set_shrink_end_child(false);
        mlt_viewer_paned.set_position(320);

        Self::install_mlt_tree_view(
            &self.mlt_tree_view,
            &self.mlt_tree_store,
            &self.mlt_viewer_list,
            &self.window,
            &self.state,
        );
        Self::refresh_mlt_collection_tree(&self.mlt_tree_store, &self.state);
        self.mlt_tree_view.expand_all();

        let notebook = gtk::Notebook::new();
        notebook.set_hexpand(true);
        notebook.set_vexpand(true);
        notebook.append_page(
            &mlt_viewer_paned,
            Some(&gtk::Label::new(Some("MLT File Viewer"))),
        );
        let mlt_tree_store = self.mlt_tree_store.clone();
        let mlt_tree_view = self.mlt_tree_view.clone();
        let state = self.state.clone();
        notebook.connect_switch_page(move |_, _, page_num| {
            if page_num == 1 {
                MainWindow::refresh_mlt_collection_tree(&mlt_tree_store, &state);
                mlt_tree_view.expand_all();
            }
        });

        self.loading_spinner.add_css_class("loading-spinner");
        self.loading_spinner.set_halign(gtk::Align::Center);
        self.loading_spinner.set_valign(gtk::Align::Center);
        Self::set_loading(&self.loading_spinner, false);

        self.overlay.set_child(Some(&main_view_box));
        self.overlay.add_overlay(&self.loading_spinner);
        notebook.prepend_page(&self.overlay, Some(&gtk::Label::new(Some("Main View"))));
        notebook.set_current_page(Some(0));
        self.v_box.append(&notebook);

        Self::install_file_menu(
            app,
            &self.editor_list,
            &self.mlt_tree_store,
            &self.window,
            &self.state,
            &self.layer_windows,
            &self.editor_view_container,
            &self.loading_spinner,
            &add_button,
        );

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
    match main.init(app, 1280, 960) {
        Ok(_) => {
            main.run();
        }
        Err(e) => {
            println!("{}", e);
        }
    }
}
