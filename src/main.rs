mod webview;

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use cosmic::app::{self, Core, Settings};
use cosmic::iced::Length;
use cosmic::iced::Subscription;
use cosmic::iced::widget::column;
use cosmic::iced::widget::progress_bar;
use cosmic::prelude::*;
use cosmic::widget::menu::{bar, items, root, Action, Item, KeyBind, Tree};
use cosmic::widget::row;
use cosmic::widget::text;
use cosmic::widget::text_input;
use cosmic::{executor, iced, widget};
use servo::LoadStatus;
use url::Url;
use webview::{ServoEvent, WebViewMessage};

/// Messages from the UI
#[derive(Clone, Debug)]
enum Message {
    UrlChanged(String),
    Go,
    Back,
    Forward,
    Exit,
    /// Webview widget messages
    WebView(WebViewMessage),
}

impl From<WebViewMessage> for Message {
    fn from(msg: WebViewMessage) -> Self {
        Message::WebView(msg)
    }
}

/// Menu actions for the menubar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MenuActionKind {
    Exit,
}

impl Action for MenuActionKind {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuActionKind::Exit => Message::Exit,
        }
    }
}

/// Manages browser navigation history.
#[derive(Default)]
struct History {
    entries: Vec<Url>,
    current: usize,
}

impl History {
    fn can_go_back(&self) -> bool {
        self.current > 0
    }

    fn can_go_forward(&self) -> bool {
        self.current < self.entries.len().saturating_sub(1)
    }
}

/// Groups browser state fields updated by Servo events.
struct BrowserState {
    url: String,
    title: Option<String>,
    load_status: LoadStatus,
    status_text: Option<String>,
}

struct App {
    core: Core,
    browser: BrowserState,
    history: History,
    webview_state: Rc<RefCell<webview::WebViewState>>,
}

impl cosmic::Application for App {
    type Message = Message;
    type Executor = executor::Default;
    type Flags = ();
    const APP_ID: &'static str = "nexos.Cosmonaut";

    fn init(core: Core, _flags: Self::Flags) -> (Self, app::Task<Message>) {
        let initial_url = "https://github.com/nexoscp/";

        // Build the webview using the extracted module
        let webview_state = Rc::new(RefCell::new(
            webview::Builder::default()
                .initial_url(initial_url)
                .transparent_background(true)
                .size(800, 600)
                .build(),
        ));

        (
            Self {
                core,
                browser: BrowserState {
                    url: initial_url.to_string(),
                    title: None,
                    load_status: LoadStatus::Complete,
                    status_text: None,
                },
                history: History::default(),
                webview_state,
            },
            app::Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        // Use the webview module's subscription for periodic ticks
        webview::subscription(&self.webview_state.borrow()).map(Message::WebView)
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::UrlChanged(u) => {
                self.browser.url = u;
                app::Task::none()
            }
            Message::Go => {
                webview::load_url(&mut self.webview_state.borrow_mut(), &self.browser.url);
                app::Task::none()
            }
            Message::Back => {
                if self.history.can_go_back() {
                    webview::go_back(&mut self.webview_state.borrow_mut(), 1);
                }
                app::Task::none()
            }
            Message::Forward => {
                if self.history.can_go_forward() {
                    webview::go_forward(&mut self.webview_state.borrow_mut(), 1);
                }
                app::Task::none()
            }
            Message::Exit => {
                std::process::exit(0);
            }
            Message::WebView(webview_msg) => {
                // Process webview messages and handle any Servo events
                let events = webview::update(&mut self.webview_state.borrow_mut(), webview_msg);
                for event in events {
                    self.handle_servo_event(event);
                }
                app::Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // Menu bar
        let key_binds: HashMap<KeyBind, MenuActionKind> = HashMap::new();
        let file_menu = items(&key_binds, vec![
            Item::Button("Exit", None, MenuActionKind::Exit),
        ]);
        let file_root: Element<'static, Message> = root("File").into();
        let menu_bar_widget = bar(vec![
            Tree::with_children(
                file_root,
                file_menu,
            ),
        ]);
        let menu_bar_element: Element<'_, Message> = menu_bar_widget.into();

        // Navigation buttons
        let back_button = widget::button::text("←")
            .on_press_maybe(self.history.can_go_back().then_some(Message::Back))
            .padding(4);
        let forward_button = widget::button::text("→")
            .on_press_maybe(self.history.can_go_forward().then_some(Message::Forward))
            .padding(4);

        // URL bar with adaptive placeholder
        let placeholder = if self.browser.url.starts_with("http://") {
            "http://"
        } else {
            "https://"
        };
        let url_input = text_input(placeholder, &self.browser.url)
            .on_input(Message::UrlChanged)
            .on_submit(|_| Message::Go);
        let go_button = widget::button::text("Go").on_press(Message::Go);

        // Title display
        let title_text = self.browser.title.as_deref().unwrap_or("");

        let url_bar = row![back_button, forward_button, url_input, go_button,]
            .padding(8)
            .spacing(8);

        // Progress bar - hidden when loading is complete
        let progress: Element<'_, Message> = match &self.browser.load_status {
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

        // Webview content - centered in available space
        let webview_widget = webview::WebViewWidget::new(Rc::clone(&self.webview_state));
        let webview_content: Element<'_, Message> = widget::container(webview_widget)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();

        // Status bar
        let status_text = self.browser.status_text.as_deref().unwrap_or("");
        let status_bar = row![text(status_text)]
            .padding(4)
            .align_y(cosmic::iced::Alignment::Center);

        column![
            menu_bar_element,
            row![text(title_text)].padding(4),
            url_bar,
            progress,
            webview_content,
            status_bar
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }
}

impl App {
    /// Handle events from the Servo delegate
    fn handle_servo_event(&mut self, event: ServoEvent) {
        match event {
            ServoEvent::UrlChanged(url) => {
                self.browser.url = url.to_string();
            }
            ServoEvent::TitleChanged(title) => {
                self.browser.title = title;
            }
            ServoEvent::LoadStatusChanged(status) => {
                self.browser.load_status = status;
            }
            ServoEvent::StatusTextChanged(status) => {
                self.browser.status_text = status;
            }
            ServoEvent::HistoryChanged(entries, current) => {
                self.history.entries = entries;
                self.history.current = current;
            }
            ServoEvent::Crashed(reason, _backtrace) => {
                eprintln!("Servo crashed: {}", reason);
            }
            // Cursor, fullscreen, and frame events are not yet handled
            ServoEvent::CursorChanged(_)
            | ServoEvent::FrameReady
            | ServoEvent::FullscreenChanged(_) => {}
        }
    }
}

fn main() -> iced::Result {
    app::run::<App>(Settings::default(), ())
}
