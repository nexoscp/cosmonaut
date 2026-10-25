mod webview;

use cosmic::app::{self, Core, Settings};
use cosmic::iced::Length;
use cosmic::iced::Subscription;
use cosmic::iced::widget::column;
use cosmic::iced::widget::progress_bar;
use cosmic::prelude::*;
use cosmic::widget::row;
use cosmic::widget::text;
use cosmic::widget::text_input;
use cosmic::{executor, iced, widget};
use servo::LoadStatus;
use std::cell::RefCell;
use std::rc::Rc;
use url::Url;
use webview::{ServoEvent, WebViewMessage};

/// Messages from the UI
#[derive(Clone, Debug)]
enum Message {
    UrlChanged(String),
    Go,
    Back,
    Forward,
    /// Webview widget messages
    WebView(WebViewMessage),
}

impl From<WebViewMessage> for Message {
    fn from(msg: WebViewMessage) -> Self {
        Message::WebView(msg)
    }
}

struct App {
    core: Core,
    url: String,
    title: Option<String>,
    load_status: LoadStatus,
    status_text: Option<String>,
    history_entries: Vec<Url>,
    history_current: usize,
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
                url: initial_url.to_string(),
                title: None,
                load_status: LoadStatus::Complete,
                status_text: None,
                history_entries: Vec::new(),
                history_current: 0,
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
                self.url = u;
                app::Task::none()
            }
            Message::Go => {
                webview::load_url(&mut self.webview_state.borrow_mut(), &self.url);
                app::Task::none()
            }
            Message::Back => {
                if self.history_current > 0 {
                    webview::go_back(&mut self.webview_state.borrow_mut(), 1);
                }
                app::Task::none()
            }
            Message::Forward => {
                if self.history_current < self.history_entries.len().saturating_sub(1) {
                    webview::go_forward(&mut self.webview_state.borrow_mut(), 1);
                }
                app::Task::none()
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
        // Navigation buttons
        let back_button = widget::button::text("←")
            .on_press_maybe(can_go_back(self).then_some(Message::Back))
            .padding(4);
        let forward_button = widget::button::text("→")
            .on_press_maybe(can_go_forward(self).then_some(Message::Forward))
            .padding(4);

        // URL bar
        let url_input = text_input("https://", &self.url)
            .on_input(Message::UrlChanged)
            .on_submit(|_| Message::Go);
        let go_button = widget::button::text("Go").on_press(Message::Go);

        // Title display
        let title_text = self.title.as_deref().unwrap_or("");

        let url_bar = row![back_button, forward_button, url_input, go_button,]
            .padding(8)
            .spacing(8);

        // Progress bar
        let progress: Element<'_, Message> = match &self.load_status {
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

        // Webview content - use the Widget implementation
        let webview_widget = webview::WebViewWidget::new(Rc::clone(&self.webview_state));
        let webview_content: Element<'_, Message> = webview_widget.into();

        // Status bar
        let status_text = self.status_text.as_deref().unwrap_or("");
        let status_bar = row![text(status_text)]
            .padding(4)
            .align_y(cosmic::iced::Alignment::Center);

        column![
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
                self.url = url.to_string();
            }
            ServoEvent::TitleChanged(title) => {
                self.title = title;
            }
            ServoEvent::LoadStatusChanged(status) => {
                self.load_status = status;
            }
            ServoEvent::StatusTextChanged(status) => {
                self.status_text = status;
            }
            ServoEvent::CursorChanged(_cursor) => {
                // Cursor changes could be handled here in the future
            }
            ServoEvent::HistoryChanged(entries, current) => {
                self.history_entries = entries;
                self.history_current = current;
            }
            ServoEvent::FrameReady => {
                // Frame ready is handled by the update function
            }
            ServoEvent::Crashed(reason, _backtrace) => {
                eprintln!("Servo crashed: {}", reason);
            }
            ServoEvent::FullscreenChanged(_is_fullscreen) => {
                // Fullscreen changes could be handled here in the future
            }
        }
    }
}

fn main() -> iced::Result {
    app::run::<App>(Settings::default(), ())
}

/// Check if we can navigate back
fn can_go_back(app: &App) -> bool {
    app.history_current > 0
}

/// Check if we can navigate forward
fn can_go_forward(app: &App) -> bool {
    app.history_current < app.history_entries.len().saturating_sub(1)
}
