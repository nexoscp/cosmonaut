mod webview;
mod menu;
mod message;
mod about;

use std::collections::HashMap;

use cosmic::app::{self, Core, Settings};
use cosmic::iced::Length;
use cosmic::iced::Subscription;
use cosmic::iced::event;
use cosmic::iced::widget::column;
use cosmic::iced::widget::progress_bar;
use cosmic::iced::window;
use cosmic::prelude::*;
use cosmic::widget::menu::{Item, KeyBind, Tree, bar, items, root};
use cosmic::widget::row;
use webview::WindowState;
use cosmic::widget::tab_bar;
use cosmic::widget::text;
use cosmic::widget::text_input;
use cosmic::{executor, iced, widget};
use menu::MenuActionKind;
use message::Message;
use webview::{segmented_button, ServoEvent, WebViewMessage};
use servo::LoadStatus;


struct App {
    core: Core,
    /// Multi-window state: maps window ID to its window state
    windows: HashMap<window::Id, WindowState>,
    /// About page data for the context drawer
    about: widget::about::About,
}

impl cosmic::Application for App {
    type Message = Message;
    type Executor = executor::Default;
    type Flags = ();
    const APP_ID: &'static str = "nexos.Cosmonaut";

    fn init(core: Core, _flags: Self::Flags) -> (Self, app::Task<Message>) {
        let initial_url = "https://github.com/nexoscp/";
        let main_window_id = core.main_window_id().unwrap_or(window::Id::RESERVED);

        let mut windows = HashMap::new();
        windows.insert(main_window_id, WindowState::new(initial_url));

        let about = about::about();

        (
            Self {
                core,
                windows,
                about,
            },
            app::Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        // Listen for window lifecycle events
        let window_events = event::listen_with(|event, _, id| {
            if let iced::Event::Window(window_event) = event {
                match window_event {
                    window::Event::CloseRequested => Some(Message::CloseWindow(id)),
                    window::Event::Opened { .. } => Some(Message::WindowOpened(id)),
                    window::Event::Closed => Some(Message::WindowClosed(id)),
                    _ => None,
                }
            } else {
                None
            }
        });

        // Single tick subscription to spin all webview event loops
        // Avoids per-tab subscriptions with capturing closures (iced doesn't allow .map() to capture)
        let tick = webview::subscription()
            .map(|_| Message::Tick);

        Subscription::batch(vec![window_events, tick])
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::UrlChanged(u) => {
                // Only update the display URL; navigation is triggered by Go (Enter or button press)
                if let Some(win) = self.get_focused_window() {
                    win.url = u;
                }
                app::Task::none()
            }
            Message::Go => {
                // Navigate to the URL currently in the address bar
                if let Some(win) = self.get_focused_window() {
                    if let Some(webview_state) = win.current_webview() {
                        let servo = {
                            let state_ref = webview_state.borrow();
                            std::rc::Rc::clone(&state_ref.servo)
                        };
                        let mut state = webview_state.borrow_mut();
                        webview::load_url(&mut state.webview, &servo, &win.url);
                    }
                }
                app::Task::none()
            }
            Message::Back => {
                if let Some(win) = self.get_focused_window() {
                    if let Some(webview_state) = win.current_webview() {
                        let servo = {
                            let state_ref = webview_state.borrow();
                            std::rc::Rc::clone(&state_ref.servo)
                        };
                        let mut state = webview_state.borrow_mut();
                        if webview::can_go_back(&state.webview) {
                            webview::go_back(&mut state.webview, &servo, 1);
                        }
                    }
                }
                app::Task::none()
            }
            Message::Forward => {
                if let Some(win) = self.get_focused_window() {
                    if let Some(webview_state) = win.current_webview() {
                        let servo = {
                            let state_ref = webview_state.borrow();
                            std::rc::Rc::clone(&state_ref.servo)
                        };
                        let mut state = webview_state.borrow_mut();
                        if webview::can_go_forward(&state.webview) {
                            webview::go_forward(&mut state.webview, &servo, 1);
                        }
                    }
                }
                app::Task::none()
            }
            Message::NewTab => {
                if let Some(win) = self.get_focused_window() {
                    win.create_new_tab();
                }
                app::Task::none()
            }
            Message::TabActivated(tab_id) => {
                if let Some(win) = self.get_focused_window() {
                    win.tab_id = tab_id;
                    win.tab_model.activate(tab_id);
                    // Sync the URL from the activated tab's webview
                    if let Some(webview_state) = win.current_webview() {
                        win.url = webview::current_url(&webview_state.borrow().webview);
                    }
                }
                app::Task::none()
            }
            Message::TabReorder(event) => {
                if let Some(win) = self.get_focused_window() {
                    let mut tab_model = std::mem::take(&mut win.tab_model);
                    tab_model.reorder(event.dragged, event.target, event.position);
                    win.tab_model = tab_model;
                }
                app::Task::none()
            }
            Message::Exit => {
                std::process::exit(0);
            }
            Message::Tick => {
                // Process tick for all windows
                for (_win_id, win) in self.windows.iter_mut() {
                    let events = webview::update(win, WebViewMessage::Tick);
                    for evt in events {
                        match evt {
                            ServoEvent::TitleChanged(title) => {
                                let tab_title = title.unwrap_or_else(|| "New Tab".to_string());
                                win.tab_model.text_set(win.tab_id, tab_title);
                            }
                            ServoEvent::LoadStatusChanged(status) => {
                                win.load_status = status;
                            }
                            ServoEvent::StatusTextChanged(status) => {
                                win.status_text = status;
                            }
                            ServoEvent::Crashed(reason, _backtrace) => {
                                eprintln!("Servo crashed: {}", reason);
                            }
                            ServoEvent::UrlChanged(url) => {
                                win.url = url.to_string();
                            }
                            ServoEvent::CursorChanged(_) | ServoEvent::FrameReady | ServoEvent::FullscreenChanged(_) => {}
                        }
                    }
                }
                app::Task::none()
            }
            Message::WebView(_tab_id, _webview_msg) => {
                // Widget-emitted messages (mouse/scroll) are handled by the widget's on_message
                app::Task::none()
            }
            Message::WindowOpened(id) => {
                // If this window doesn't have state yet, it was opened from a tab popout
                // and should already have state from the popout handler
                if !self.windows.contains_key(&id) {
                    // Fallback: create empty window state
                    self.windows.insert(id, WindowState::new(""));
                }
                app::Task::none()
            }
            Message::WindowClosed(id) => {
                self.windows.remove(&id);
                app::Task::none()
            }
            Message::WindowMoved(temp_id, real_id) => {
                // Move window state from temporary ID to the real ID
                if let Some(win_state) = self.windows.remove(&temp_id) {
                    self.windows.insert(real_id, win_state);
                }
                app::Task::none()
            }
            Message::CloseWindow(id) => {
                window::close(id)
            }
            Message::TabPoppedOut(tab_id) => {
                // Extract the tab from the focused window and create a new window
                if let Some(source_win) = self.get_focused_window() {
                    // Get the tab title before removing
                    let tab_title = source_win.tab_model.text(tab_id)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "New Tab".to_string());

                    // Extract the tab's webview from the source window
                    let extracted = match source_win.extract_tab(tab_id) {
                        Some(state) => state,
                        None => return app::Task::none(),
                    };
                    let url = webview::current_url(&extracted.borrow().webview);

                    // Build a new window state with the extracted webview
                    let mut tab_model = segmented_button::SingleSelectModel::default();
                    let new_tab_id = tab_model.insert().text(tab_title).id();
                    tab_model.activate(new_tab_id);

                    let mut new_tab_webview = segmented_button::SecondaryMap::new();
                    new_tab_webview.insert(new_tab_id, extracted);

                    let new_win = WindowState {
                        servo: std::rc::Rc::clone(&source_win.servo),
                        tab_webview: new_tab_webview,
                        url,
                        tab_model,
                        tab_id: new_tab_id,
                        load_status: LoadStatus::Complete,
                        status_text: None,
                    };

                    // window::open returns (temporary_id, Task<real_id>)
                    let (temp_id, open_task) = window::open(window::Settings {
                        position: Default::default(),
                        exit_on_close_request: false,
                        decorations: false,
                        ..Default::default()
                    });

                    self.windows.insert(temp_id, new_win);
                    open_task.map(move |real_id| cosmic::Action::App(Message::WindowMoved(temp_id, real_id)))
                } else {
                    app::Task::none()
                }
            }
            Message::ContextDrawerToggled => {
                // Toggle the context drawer visibility
                self.set_show_context(!self.core.window.show_context);
                app::Task::none()
            }
            Message::AboutUrlPressed(url) => {
                // Navigate the focused window to the clicked URL
                if let Some(win) = self.get_focused_window() {
                    if let Some(webview_state) = win.current_webview() {
                        let servo = {
                            let state_ref = webview_state.borrow();
                            std::rc::Rc::clone(&state_ref.servo)
                        };
                        let mut state = webview_state.borrow_mut();
                        webview::load_url(&mut state.webview, &servo, &url);
                    }
                }
                app::Task::none()
            }
        }
    }

    fn context_drawer(&self) -> Option<app::ContextDrawer<'_, Message>> {
        if !self.core.window.show_context {
            return None;
        }
        let about_widget = widget::about(&self.about, |url| Message::AboutUrlPressed(url.to_string()));
        Some(app::context_drawer(about_widget, Message::ContextDrawerToggled)
            .title("About Cosmonaut"))
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        let win = self.windows.get(&id).expect("window exists");
        let is_main = id == self.core.main_window_id().unwrap_or(window::Id::RESERVED);

        // Menu bar
        let key_binds: HashMap<KeyBind, MenuActionKind> = HashMap::new();
        let file_menu = items(&key_binds, vec![
            Item::Button("New Tab", None, MenuActionKind::NewTab),
        ]);
        let help_menu = items(&key_binds, vec![
            Item::Button("About", None, MenuActionKind::About),
            Item::Button("Exit", None, MenuActionKind::Exit),
        ]);
        let file_root: Element<'static, Message> = root("File").into();
        let help_root: Element<'static, Message> = root("Help").into();
        let menu_bar_widget = bar(vec![
            Tree::with_children(
                file_root,
                file_menu,
            ),
            Tree::with_children(
                help_root,
                help_menu,
            ),
        ]);
        let menu_bar_element: Element<'_, Message> = menu_bar_widget.into();

        // Query current tab's webview for navigation state
        let (can_back, can_forward) = {
            if let Some(webview_state) = win.current_webview() {
                let state = webview_state.borrow();
                (webview::can_go_back(&state.webview), webview::can_go_forward(&state.webview))
            } else {
                (false, false)
            }
        };

        // URL bar uses the window's URL field
        let url = &win.url;

        // Tab bar with page title in the tab header
        // Enables tab drag-and-drop for reordering and pulling tabs out
        let tab_bar_widget: Element<'_, Message> = tab_bar::horizontal(&win.tab_model)
            .on_activate(|tab_id| Message::TabActivated(tab_id))
            .enable_tab_drag("cosmonaut/tab".to_string())
            .on_reorder(|event| Message::TabReorder(event))
            .id(segmented_button::Id::new("tab-bar"))
            .into();

        // Navigation buttons
        let back_button = widget::button::text("←")
            .on_press_maybe(can_back.then_some(Message::Back))
            .padding(4);
        let forward_button = widget::button::text("→")
            .on_press_maybe(can_forward.then_some(Message::Forward))
            .padding(4);

        // URL bar with adaptive placeholder
        let placeholder = if url.starts_with("http://") {
            "http://"
        } else {
            "https://"
        };
        let url_input = text_input(placeholder, url)
            .on_input(Message::UrlChanged)
            .on_submit(|_| Message::Go);
        let url_bar = row![back_button, forward_button, url_input, widget::button::text("Go").on_press(Message::Go),]
            .padding(8)
            .spacing(8);

        // Progress bar - hidden when loading is complete
        let progress: Element<'_, Message> = match &win.load_status {
            LoadStatus::Started | LoadStatus::HeadParsed => {
                let pb = progress_bar(0.0..=1.0, 0.5);
                widget::container(pb)
                    .width(Length::Fill)
                    .height(4)
                    .into()
            }
            LoadStatus::Complete => {
                widget::container(text(""))
                    .width(Length::Fill)
                    .height(4)
                    .into()
            }
        };

        // Webview content
        let webview_content: Element<'_, Message> = {
            let webview_widget = webview::WebViewWidget::new(win);
            widget::container(webview_widget)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        };

        // Status bar
        let status_text = win.status_text.as_deref().unwrap_or("");
        let status_bar = widget::container(text(status_text))
            .padding(4);

        // Tab content: URL bar, progress, webview, and status bar
        let tab_content = column![
            url_bar,
            progress,
            webview_content,
            status_bar
        ]
        .width(Length::Fill)
        .height(Length::Fill);

        let window_content = column![
            menu_bar_element,
            tab_bar_widget,
            tab_content
        ]
        .width(Length::Fill)
        .height(Length::Fill);

        if is_main {
            window_content.into()
        } else {
            // Non-main windows get a header bar for closing
            let focused = self.core.focused_window().map(|i| i == id).unwrap_or_default();
            let header = widget::header_bar()
                .on_close(Message::CloseWindow(id))
                .focused(focused);

            column![header, window_content]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let main_id = self.core.main_window_id().unwrap_or(window::Id::RESERVED);
        self.view_window(main_id)
    }

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }
}

impl App {
    /// Get a mutable reference to the focused window's state
    fn get_focused_window(&mut self) -> Option<&mut WindowState> {
        let focused_id = self.core.focused_window()
            .unwrap_or_else(|| self.core.main_window_id().unwrap_or(window::Id::RESERVED));
        self.windows.get_mut(&focused_id)
    }
}

fn main() -> iced::Result {
    app::run::<App>(Settings::default(), ())
}
