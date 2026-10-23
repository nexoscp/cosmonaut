use cosmic::app::{self, Core, Settings};
use cosmic::iced::Length;
use cosmic::iced::Point;
use cosmic::iced::Subscription;
use cosmic::iced::mouse::ScrollDelta;
use cosmic::iced::time::{Duration, every};
use cosmic::iced::widget::column;
use cosmic::iced::widget::image;
use cosmic::iced::widget::image::Handle;
use cosmic::iced::widget::progress_bar;
use cosmic::prelude::*;
use cosmic::widget::row;
use cosmic::widget::text;
use cosmic::widget::text_input;
use cosmic::{executor, iced, widget};
use dpi::PhysicalSize;
use servo::{
    Cursor, DeviceIntPoint, DeviceIntRect, DevicePoint, InputEvent, LoadStatus, MouseButton,
    MouseButtonAction, MouseButtonEvent, MouseMoveEvent, RenderingContext, Servo, ServoBuilder,
    SoftwareRenderingContext, WebView, WebViewBuilder, WebViewDelegate, WebViewPoint, WheelDelta,
    WheelEvent, WheelMode,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use url::Url;

/// Apply transparency to RGBA pixel data to allow frost effect to show through
fn apply_transparency(pixels: &[u8], alpha: u8) -> Vec<u8> {
    let mut result = Vec::with_capacity(pixels.len());
    let alpha_factor = alpha as f32 / 255.0;
    for chunk in pixels.chunks_exact(4) {
        let r = chunk[0] as f32;
        let g = chunk[1] as f32;
        let b = chunk[2] as f32;
        let a = chunk[3] as f32;
        // Apply transparency while preserving the original alpha
        result.push(r as u8);
        result.push(g as u8);
        result.push(b as u8);
        result.push((a * alpha_factor) as u8);
    }
    result
}

/// Events from Servo's delegate callbacks
#[derive(Clone, Debug)]
enum ServoEvent {
    UrlChanged(Url),
    TitleChanged(Option<String>),
    LoadStatusChanged(LoadStatus),
    StatusTextChanged(Option<String>),
    CursorChanged(Cursor),
    HistoryChanged(Vec<Url>, usize),
    FrameReady,
    Crashed(String, Option<String>),
    FullscreenChanged(bool),
}

/// Messages from the UI
#[derive(Clone, Debug)]
enum Message {
    UrlChanged(String),
    Go,
    Back,
    Forward,
    /// Periodic tick to spin Servo's event loop
    Tick,
    /// Mouse button pressed on webview
    MousePressed(MouseButton),
    /// Mouse button released on webview
    MouseReleased(MouseButton),
    /// Mouse moved over webview
    MouseMoved(f32, f32),
    /// Mouse wheel scrolled over webview
    Scrolled(f64, f64),
}

struct App {
    core: Core,
    url: String,
    title: Option<String>,
    load_status: LoadStatus,
    status_text: Option<String>,
    history_entries: Vec<Url>,
    history_current: usize,
    servo: Rc<Servo>,
    rendering_context: Rc<SoftwareRenderingContext>,
    webview: WebView,
    frame_handle: Option<Handle>,
    /// Atomic flag set by the delegate when a frame is ready
    frame_ready_flag: Arc<Cell<bool>>,
    /// Channel receiver for Servo events
    servo_event_receiver: iced::futures::channel::mpsc::UnboundedReceiver<ServoEvent>,
    /// Current mouse position relative to the webview
    mouse_position: (f32, f32),
}

/// WebViewDelegate that sends events through a channel
struct ServoDelegate {
    flag: Arc<Cell<bool>>,
    sender: Rc<RefCell<iced::futures::channel::mpsc::UnboundedSender<ServoEvent>>>,
}

impl WebViewDelegate for ServoDelegate {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.flag.set(true);
        let _ = self.sender.borrow_mut().start_send(ServoEvent::FrameReady);
    }

    fn notify_url_changed(&self, _webview: WebView, url: Url) {
        let _ = self.sender.borrow_mut().start_send(ServoEvent::UrlChanged(url));
    }

    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        let _ = self.sender.borrow_mut().start_send(ServoEvent::TitleChanged(title));
    }

    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        let _ = self.sender.borrow_mut().start_send(ServoEvent::LoadStatusChanged(status));
    }

    fn notify_status_text_changed(&self, _webview: WebView, status: Option<String>) {
        let _ = self.sender.borrow_mut().start_send(ServoEvent::StatusTextChanged(status));
    }

    fn notify_cursor_changed(&self, _webview: WebView, cursor: Cursor) {
        let _ = self.sender.borrow_mut().start_send(ServoEvent::CursorChanged(cursor));
    }

    fn notify_history_changed(
        &self,
        _webview: WebView,
        entries: Vec<Url>,
        current: usize,
    ) {
        let _ = self.sender.borrow_mut().start_send(ServoEvent::HistoryChanged(entries, current));
    }

    fn notify_crashed(
        &self,
        _webview: WebView,
        reason: String,
        backtrace: Option<String>,
    ) {
        let _ = self.sender.borrow_mut().start_send(ServoEvent::Crashed(reason, backtrace));
    }

    fn notify_fullscreen_state_changed(&self, _webview: WebView, is_fullscreen: bool) {
        let _ = self.sender.borrow_mut().start_send(ServoEvent::FullscreenChanged(is_fullscreen));
    }
}

impl cosmic::Application for App {
    type Message = Message;
    type Executor = executor::Default;
    type Flags = ();
    const APP_ID: &'static str = "nexos.Cosmonaut";

    fn init(core: Core, _flags: Self::Flags) -> (Self, app::Task<Message>) {
        // Initialize the Servo engine
        let servo = ServoBuilder::default().build();
        servo.setup_logging();

        // Create a software rendering context with an initial size
        let initial_size = PhysicalSize { width: 800, height: 600 };
        let rendering_context = SoftwareRenderingContext::new(initial_size)
            .expect("Failed to create SoftwareRenderingContext");
        let rendering_context_rc = Rc::new(rendering_context);

        // Create a flag for frame-ready signaling
        let frame_ready_flag = Arc::new(Cell::new(false));

        // Create channel for Servo events
        let (servo_event_sender, servo_event_receiver) =
            iced::futures::channel::mpsc::unbounded::<ServoEvent>();

        // Create a WebView with the delegate
        let delegate = Rc::new(ServoDelegate {
            flag: frame_ready_flag.clone(),
            sender: Rc::new(RefCell::new(servo_event_sender)),
        });
        let webview = WebViewBuilder::new(&servo, rendering_context_rc.clone())
            .delegate(delegate)
            .build();

        // Load the initial URL
        let initial_url = "https://github.com/nexoscp/";
        if let Ok(u) = Url::parse(initial_url) {
            webview.load(u);
        }

        let servo_rc = Rc::new(servo);

        // Spin the event loop until the first frame is ready
        loop {
            servo_rc.spin_event_loop();
            if frame_ready_flag.get() {
                frame_ready_flag.set(false);
                break;
            }
        }

        // Paint and capture the initial frame
        webview.paint();
        let mut frame_handle = None;
        {
            rendering_context_rc.present();
            let size = rendering_context_rc.size();
            let rect = DeviceIntRect::new(
                DeviceIntPoint::origin(),
                DeviceIntPoint::new(size.width as i32, size.height as i32),
            );
            if let Some(img) = rendering_context_rc.read_to_image(rect) {
                // Apply transparency to allow frost effect to show through
                let pixels = apply_transparency(img.as_raw(), 200);
                frame_handle = Some(Handle::from_rgba(size.width, size.height, pixels));
            }
        }

        (
            Self {
                core,
                url: initial_url.to_string(),
                title: None,
                load_status: LoadStatus::Complete,
                status_text: None,
                history_entries: Vec::new(),
                history_current: 0,
                servo: servo_rc,
                rendering_context: rendering_context_rc,
                webview,
                frame_handle,
                frame_ready_flag,
                servo_event_receiver,
                mouse_position: (0.0, 0.0),
            },
            app::Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        // Periodic tick to keep Servo's event loop running
        every(Duration::from_millis(16)).map(|_| Message::Tick)
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::UrlChanged(u) => {
                self.url = u;
                app::Task::none()
            }
            Message::Go => {
                let url_str = if self.url.starts_with("http://") || self.url.starts_with("https://")
                {
                    self.url.clone()
                } else {
                    format!("https://{}", self.url)
                };
                if let Ok(u) = Url::parse(&url_str) {
                    self.webview.load(u);
                }
                // Spin once to start processing
                self.servo.spin_event_loop();
                app::Task::none()
            }
            Message::Back => {
                if self.history_current > 0 {
                    self.webview.go_back(1);
                    self.servo.spin_event_loop();
                }
                app::Task::none()
            }
            Message::Forward => {
                if self.history_current < self.history_entries.len().saturating_sub(1) {
                    self.webview.go_forward(1);
                    self.servo.spin_event_loop();
                }
                app::Task::none()
            }
            Message::Tick => {
                // Spin Servo's event loop
                self.servo.spin_event_loop();

                // Process any pending Servo events from the channel
                while let Ok(event) = self.servo_event_receiver.try_recv() {
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
                            // Frame ready is also handled via the flag below
                        }
                        ServoEvent::Crashed(reason, _backtrace) => {
                            eprintln!("Servo crashed: {}", reason);
                        }
                        ServoEvent::FullscreenChanged(_is_fullscreen) => {
                            // Fullscreen changes could be handled here in the future
                        }
                    }
                }

                // Check if a frame is ready
                if self.frame_ready_flag.get() {
                    self.frame_ready_flag.set(false);

                    // Paint and capture the frame
                    self.webview.paint();
                    self.rendering_context.present();
                    let size = self.rendering_context.size();
                    let rect = DeviceIntRect::new(
                        DeviceIntPoint::origin(),
                        DeviceIntPoint::new(size.width as i32, size.height as i32),
                    );
                    if let Some(img) = self.rendering_context.read_to_image(rect) {
                        // Apply transparency to allow frost effect to show through
                        let pixels = apply_transparency(img.as_raw(), 200);
                        self.frame_handle =
                            Some(Handle::from_rgba(size.width, size.height, pixels));
                    }
                }
                app::Task::none()
            }
            Message::MousePressed(button) => {
                let (x, y) = self.mouse_position;
                let point = WebViewPoint::Device(DevicePoint::new(x, y));
                let event = InputEvent::MouseButton(MouseButtonEvent::new(
                    MouseButtonAction::Down,
                    button,
                    point,
                ));
                self.webview.notify_input_event(event);
                self.servo.spin_event_loop();
                app::Task::none()
            }
            Message::MouseReleased(button) => {
                let (x, y) = self.mouse_position;
                let point = WebViewPoint::Device(DevicePoint::new(x, y));
                let event = InputEvent::MouseButton(MouseButtonEvent::new(
                    MouseButtonAction::Up,
                    button,
                    point,
                ));
                self.webview.notify_input_event(event);
                self.servo.spin_event_loop();
                app::Task::none()
            }
            Message::MouseMoved(x, y) => {
                self.mouse_position = (x, y);
                let point = WebViewPoint::Device(DevicePoint::new(x, y));
                let event = InputEvent::MouseMove(MouseMoveEvent::new(point));
                self.webview.notify_input_event(event);
                app::Task::none()
            }
            Message::Scrolled(dx, dy) => {
                let (x, y) = self.mouse_position;
                let point = WebViewPoint::Device(DevicePoint::new(x, y));
                let event = InputEvent::Wheel(WheelEvent::new(
                    WheelDelta {
                        x: dx,
                        y: dy,
                        z: 0.0,
                        mode: WheelMode::DeltaPixel,
                    },
                    point,
                ));
                self.webview.notify_input_event(event);
                self.servo.spin_event_loop();
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

        let webview_content: Element<'_, Message> = match &self.frame_handle {
            Some(handle) => {
                let img = image::Image::new(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill);

                // Wrap in MouseArea to capture mouse events
                let interactive = widget::MouseArea::new(img)
                    .on_press(Message::MousePressed(MouseButton::Primary))
                    .on_release(Message::MouseReleased(MouseButton::Primary))
                    .on_right_press(Message::MousePressed(MouseButton::Secondary))
                    .on_right_release(Message::MouseReleased(MouseButton::Secondary))
                    .on_middle_press(Message::MousePressed(MouseButton::Auxiliary))
                    .on_middle_release(Message::MouseReleased(MouseButton::Auxiliary))
                    .on_move(|pos: Point| Message::MouseMoved(pos.x, pos.y))
                    .on_scroll(|delta: ScrollDelta| {
                        match delta {
                            ScrollDelta::Lines { x, y } => {
                                Message::Scrolled(x as f64 * 76.0, y as f64 * 76.0)
                            }
                            ScrollDelta::Pixels { x, y } => {
                                Message::Scrolled(x as f64, y as f64)
                            }
                        }
                    });

                widget::container(interactive)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            None => widget::container(text("Loading..."))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        };

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
