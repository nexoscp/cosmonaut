mod webview;
mod menu;
mod message;
mod about;
mod tab;
mod window;

use cosmic::app::{self, Core, Settings};
use cosmic::iced::Length;
use cosmic::iced::Subscription;
use cosmic::iced::event;
use cosmic::iced::widget::column;
use cosmic::iced::widget::progress_bar;
use cosmic::iced::window as iced_window;
use cosmic::prelude::*;
use cosmic::widget::menu::{Item, KeyBind, Tree, bar, items, root};
use cosmic::widget::row;
use cosmic::widget::tab_bar;
use cosmic::widget::text;
use cosmic::widget::text_input;
use cosmic::{executor, iced, widget};
use menu::MenuActionKind;
use message::Message;
use webview::WebViewMessage;
use window::{WindowState, WindowManager};
use servo::LoadStatus;

struct App {
    core: Core,
    /// Window manager for multi-window support
    window_manager: WindowManager,
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
        let main_window_id = core.main_window_id().unwrap_or(iced_window::Id::RESERVED);

        let window_manager = WindowManager::new(main_window_id, initial_url);
        let about = about::about();

        (
            Self {
                core,
                window_manager,
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
                    iced_window::Event::CloseRequested => Some(Message::CloseWindow(id)),
                    iced_window::Event::Opened { .. } => Some(Message::WindowOpened(id)),
                    iced_window::Event::Closed => Some(Message::WindowClosed(id)),
                    _ => None,
                }
            } else {
                None
            }
        });

        // Single tick subscription to spin all webview event loops
        let tick = webview::subscription()
            .map(|_| Message::Tick);

        Subscription::batch(vec![window_events, tick])
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::UrlChanged(u) => {
                if let Some(win) = self.get_focused_window() {
                    win.url = u;
                }
                app::Task::none()
            }
            Message::Go => {
                if let Some(win) = self.get_focused_window() {
                    let url = win.url.clone();
                    win.navigate(&url);
                }
                app::Task::none()
            }
            Message::Back => {
                if let Some(win) = self.get_focused_window() {
                    win.go_back();
                }
                app::Task::none()
            }
            Message::Forward => {
                if let Some(win) = self.get_focused_window() {
                    win.go_forward();
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
                    win.tabs.activate(tab_id);
                    win.url = win.tabs.active_url();
                }
                app::Task::none()
            }
            Message::TabReorder(event) => {
                if let Some(win) = self.get_focused_window() {
                    win.tabs.reorder(event);
                }
                app::Task::none()
            }
            Message::Exit => {
                std::process::exit(0);
            }
            Message::Tick => {
                for (_win_id, win) in self.window_manager.iter_mut() {
                    let events = win.update_webview(WebViewMessage::Tick);
                    for evt in events {
                        win.handle_servo_event(evt);
                    }
                }
                app::Task::none()
            }
            Message::WebView(_tab_id, _webview_msg) => {
                app::Task::none()
            }
            Message::WindowOpened(id) => {
                if !self.window_manager.contains(id) {
                    self.window_manager.add_window(id, WindowState::new(""));
                }
                app::Task::none()
            }
            Message::WindowClosed(id) => {
                self.window_manager.remove_window(id);
                app::Task::none()
            }
            Message::WindowMoved(temp_id, real_id) => {
                self.window_manager.move_window(temp_id, real_id);
                app::Task::none()
            }
            Message::CloseWindow(id) => {
                iced_window::close(id)
            }
            Message::TabPoppedOut(tab_id) => {
                let focused_id = self.core.focused_window()
                    .unwrap_or_else(|| self.core.main_window_id().unwrap_or(iced_window::Id::RESERVED));
                
                if let Some(source_win) = self.window_manager.get_window_mut(focused_id) {
                    let new_win = match source_win.extract_tab_to_window(tab_id) {
                        Some(w) => w,
                        None => return app::Task::none(),
                    };

                    let (temp_id, open_task) = iced_window::open(iced_window::Settings {
                        position: Default::default(),
                        exit_on_close_request: false,
                        decorations: false,
                        ..Default::default()
                    });

                    self.window_manager.add_window(temp_id, new_win);
                    open_task.map(move |real_id| cosmic::Action::App(Message::WindowMoved(temp_id, real_id)))
                } else {
                    app::Task::none()
                }
            }
            Message::ContextDrawerToggled => {
                self.set_show_context(!self.core.window.show_context);
                app::Task::none()
            }
            Message::AboutUrlPressed(url) => {
                if let Some(win) = self.get_focused_window() {
                    win.navigate(&url);
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

    fn view_window(&self, id: iced_window::Id) -> Element<'_, Message> {
        let win = self.window_manager.get_window(id).expect("window exists");
        let is_main = id == self.core.main_window_id().unwrap_or(iced_window::Id::RESERVED);

        // Menu bar
        let key_binds: std::collections::HashMap<KeyBind, MenuActionKind> = std::collections::HashMap::new();
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
            Tree::with_children(file_root, file_menu),
            Tree::with_children(help_root, help_menu),
        ]);
        let menu_bar_element: Element<'_, Message> = menu_bar_widget.into();

        // Navigation state
        let can_back = win.can_go_back();
        let can_forward = win.can_go_forward();

        // URL bar
        let url = &win.url;

        // Tab bar
        let tab_bar_widget: Element<'_, Message> = tab_bar::horizontal(win.tabs.model())
            .on_activate(|tab_id| Message::TabActivated(tab_id))
            .enable_tab_drag("cosmonaut/tab".to_string())
            .on_reorder(|event| Message::TabReorder(event))
            .id(webview::segmented_button::Id::new("tab-bar"))
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

        // Progress bar
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
            let webview_widget = webview::WebViewWidget::new(&win.tabs);
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

        // Tab content
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
        let main_id = self.core.main_window_id().unwrap_or(iced_window::Id::RESERVED);
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
    fn get_focused_window(&mut self) -> Option<&mut WindowState> {
        let focused_id = self.core.focused_window()
            .unwrap_or_else(|| self.core.main_window_id().unwrap_or(iced_window::Id::RESERVED));
        self.window_manager.get_focused_window(focused_id)
    }
}

fn main() -> iced::Result {
    app::run::<App>(Settings::default(), ())
}
