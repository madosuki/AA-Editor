use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;

use gtk4 as gtk;

use gtk::gio;
use gtk::gio::prelude::{ActionMapExt, ApplicationExt};
use gtk::glib;
use gtk::prelude::{
    ApplicationWindowExt, BoxExt, ButtonExt, Cast, DialogExtManual, FileChooserExt, FileExt,
    GtkApplicationExt, GtkWindowExt, ListBoxRowExt, NativeDialogExt, NativeDialogExtManual,
    TextBufferExt, TextViewExt, WidgetExt,
};
use gtk::{
    Application, ApplicationWindow, ButtonsType, CssProvider, FileChooserAction, FileChooserNative,
    MessageDialog, MessageType, ResponseType, STYLE_PROVIDER_PRIORITY_APPLICATION, gdk,
};

use anyhow::{Context, Result};
use encoding_rs::SHIFT_JIS;

use crate::project_file::ProjectFile;

const APP_TITLE: &str = "AA Editor";

#[derive(Debug, Default)]
struct ProjectState {
    current_path: Option<PathBuf>,
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
    ) -> gtk::ListBoxRow {
        let editor_row = gtk::ListBoxRow::new();
        editor_row.set_margin_top(6);
        editor_row.set_margin_bottom(6);
        editor_row.set_margin_start(8);
        editor_row.set_margin_end(8);
        editor_row.set_vexpand(false);
        editor_row.set_activatable(false);
        editor_row.set_selectable(false);

        let item_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        item_box.set_hexpand(true);
        item_box.set_vexpand(false);

        let left_pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left_pane.set_width_request(120);
        left_pane.set_valign(gtk::Align::Start);

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
        text_view.set_monospace(true);
        text_view.set_hexpand(true);
        text_view.set_vexpand(false);
        text_view.set_size_request(640, 480);
        text_view.buffer().set_text(text);
        Self::update_item_info_label(0, &text_view.buffer(), &item_info);

        let window_for_change = window.clone();
        let state_for_change = state.clone();
        let item_info_for_change = item_info.clone();
        text_view.buffer().connect_changed(move |buffer| {
            let item_number = Self::item_number_from_info_label(&item_info_for_change);
            Self::update_item_info_label(item_number, buffer, &item_info_for_change);
            Self::mark_dirty(&window_for_change, &state_for_change);
        });

        let close_button = gtk::Button::with_label("Close");
        close_button.set_valign(gtk::Align::Start);

        let row_for_delete = editor_row.clone();
        let window_for_delete = window.clone();
        let state_for_delete = state.clone();
        close_button.connect_clicked(move |_| {
            MainWindow::confirm_remove_editor_row(
                &row_for_delete,
                &window_for_delete,
                &state_for_delete,
            );
        });

        left_pane.append(&item_info);

        item_box.append(&left_pane);
        item_box.append(&text_view);
        item_box.append(&close_button);

        editor_row.set_child(Some(&item_box));
        editor_row
    }

    fn append_editor(
        editor_list: &gtk::ListBox,
        text: &str,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
    ) {
        editor_list.append(&Self::create_editor_row(text, window, state));
        Self::renumber_editors(editor_list);
    }

    fn clear_editors(editor_list: &gtk::ListBox) {
        while let Some(child) = editor_list.first_child() {
            editor_list.remove(&child);
        }
    }

    fn set_loading(spinner: &gtk::Spinner, loading: bool) {
        spinner.set_visible(loading);
        if loading {
            spinner.start();
        } else {
            spinner.stop();
        }
    }

    fn set_close_buttons_enabled(editor_list: &gtk::ListBox, enabled: bool) {
        let mut child = editor_list.first_child();

        while let Some(row_widget) = child {
            child = row_widget.next_sibling();

            let Ok(row) = row_widget.downcast::<gtk::ListBoxRow>() else {
                continue;
            };
            let Some(item_box) = row
                .child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
            else {
                continue;
            };

            let mut item_child = item_box.first_child();
            while let Some(widget) = item_child {
                item_child = widget.next_sibling();
                if let Ok(button) = widget.downcast::<gtk::Button>() {
                    button.set_sensitive(enabled);
                }
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

    fn load_project_texts_from_path(path: &Path) -> Result<Vec<String>> {
        Ok(ProjectFile::read_from_path(path)?.to_texts())
    }

    fn apply_project_texts(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        path: PathBuf,
        texts: Vec<String>,
    ) {
        Self::clear_editors(editor_list);

        if texts.is_empty() {
            Self::append_editor(editor_list, "", window, state);
        } else {
            for text in texts {
                Self::append_editor(editor_list, &text, window, state);
            }
        }

        Self::mark_clean(window, state, Some(path));
    }

    fn apply_mlt_texts(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        texts: Vec<String>,
    ) {
        for text in texts {
            Self::append_editor(editor_list, &text, window, state);
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
            let Some(item_box) = row
                .child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
            else {
                continue;
            };
            let Some(left_pane) = item_box
                .first_child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
            else {
                continue;
            };
            let Some(info_label) = left_pane
                .first_child()
                .and_then(|child| child.downcast::<gtk::Label>().ok())
            else {
                continue;
            };
            let Some(text_view) = Self::text_view_from_row_content(row.child().unwrap()) else {
                continue;
            };

            Self::update_item_info_label(index, &text_view.buffer(), &info_label);
            index += 1;
        }
    }

    fn confirm_remove_editor_row(
        row: &gtk::ListBoxRow,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
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
        dialog.run_async(move |dialog, response| {
            if response == ResponseType::Ok {
                if let Some(parent) = row
                    .parent()
                    .and_then(|parent| parent.downcast::<gtk::ListBox>().ok())
                {
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

    fn text_view_from_row_content(widget: gtk::Widget) -> Option<gtk::TextView> {
        if let Ok(text_view) = widget.clone().downcast::<gtk::TextView>() {
            return Some(text_view);
        }

        let Ok(item_box) = widget.downcast::<gtk::Box>() else {
            return None;
        };

        let mut child = item_box.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(text_view) = widget.downcast::<gtk::TextView>() {
                return Some(text_view);
            }
        }

        None
    }
    fn load_project_from_path_async(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        loading_controls: &LoadingControls,
        path: PathBuf,
    ) {
        Self::set_loading_state(loading_controls, true);

        let editor_list = editor_list.clone();
        let window = window.clone();
        let state = state.clone();
        let loading_controls = loading_controls.clone();
        glib::spawn_future_local(async move {
            let (sender, receiver) = mpsc::channel();
            let path_for_worker = path.clone();

            std::thread::spawn(move || {
                let result = MainWindow::load_project_texts_from_path(&path_for_worker)
                    .map_err(|error| error.to_string());
                let _ = sender.send(result);
            });

            glib::idle_add_local(move || match receiver.try_recv() {
                Ok(Ok(texts)) => {
                    MainWindow::apply_project_texts(
                        &editor_list,
                        &window,
                        &state,
                        path.clone(),
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
        let project_file = ProjectFile::from_texts(Self::collect_editor_texts(editor_list));
        project_file.write_to_path(path)?;
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
            path.with_extension("json")
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
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
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
        let window = window.clone();
        let state = state.clone();
        let loading_controls = loading_controls.clone();
        dialog.run_async(move |dialog, response| {
            if response == ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    MainWindow::load_project_from_path_async(
                        &editor_list,
                        &window,
                        &state,
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
            Some("MLTで出力"),
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

    fn load_mlt_from_path_async(
        editor_list: &gtk::ListBox,
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
        loading_controls: &LoadingControls,
        path: PathBuf,
    ) {
        Self::set_loading_state(loading_controls, true);

        let editor_list = editor_list.clone();
        let window = window.clone();
        let state = state.clone();
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
                    MainWindow::apply_mlt_texts(&editor_list, &window, &state, texts);
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
        let loading_controls = loading_controls.clone();
        dialog.run_async(move |dialog, response| {
            if response == ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    MainWindow::load_mlt_from_path_async(
                        &editor_list,
                        &window,
                        &state,
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
        window: &gtk::ApplicationWindow,
        state: &Rc<RefCell<ProjectState>>,
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
        let window_clone = window.clone();
        let state_clone = state.clone();
        let loading_controls_clone = loading_controls.clone();
        open_action.connect_activate(move |_, _| {
            MainWindow::show_open_dialog(
                &editor_list_clone,
                &window_clone,
                &state_clone,
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
        let loading_controls_clone = loading_controls.clone();
        import_mlt_action.connect_activate(move |_, _| {
            MainWindow::show_import_mlt_dialog(
                &editor_list_clone,
                &window_clone,
                &state_clone,
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

        self.v_box.set_halign(gtk::Align::Fill);
        self.v_box.set_valign(gtk::Align::Fill);
        self.v_box.set_hexpand(true);
        self.v_box.set_vexpand(true);

        self.install_text_style();

        self.editor_list
            .set_selection_mode(gtk::SelectionMode::None);
        self.editor_list.set_hexpand(true);
        self.editor_list.set_vexpand(true);
        Self::append_editor(&self.editor_list, "", &self.window, &self.state);
        Self::mark_clean(&self.window, &self.state, None);

        self.view_window.set_child(Some(&self.editor_list));
        self.view_window
            .set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        self.view_window.set_hexpand(true);
        self.view_window.set_vexpand(true);
        self.v_box.append(&self.view_window);

        let action_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        action_box.set_halign(gtk::Align::End);
        action_box.set_margin_top(8);
        action_box.set_margin_end(8);
        action_box.set_margin_bottom(8);

        let add_button = gtk::Button::with_label("Add");
        let editor_list = self.editor_list.clone();
        let window = self.window.clone();
        let state = self.state.clone();
        add_button.connect_clicked(move |_| {
            MainWindow::append_editor(&editor_list, "", &window, &state);
            MainWindow::mark_dirty(&window, &state);
        });

        action_box.append(&add_button);
        self.v_box.append(&action_box);

        self.loading_spinner.add_css_class("loading-spinner");
        self.loading_spinner.set_halign(gtk::Align::Center);
        self.loading_spinner.set_valign(gtk::Align::Center);
        Self::set_loading(&self.loading_spinner, false);

        self.overlay.set_child(Some(&self.v_box));
        self.overlay.add_overlay(&self.loading_spinner);

        Self::install_file_menu(
            app,
            &self.editor_list,
            &self.window,
            &self.state,
            &self.loading_spinner,
            &add_button,
        );

        self.window.set_application(Some(app));
        self.window.set_child(Some(&self.overlay));

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
