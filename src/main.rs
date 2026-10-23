use cosmic::app::{self, Core, Settings};
use cosmic::dialog::ashpd::url;
use cosmic::iced::Length;
use cosmic::iced::Point;
use cosmic::iced::Subscription;
use cosmic::iced::mouse::ScrollDelta;
use cosmic::iced::time::{Duration, every};
use cosmic::iced::widget::column;
use cosmic::iced::widget::image;
use cosmic::iced::widget::image::Handle;
use cosmic::prelude::*;
use cosmic::widget::row;
use cosmic::widget::text;
use cosmic::widget::text_input;
use cosmic::{executor, iced, widget};
use dpi::PhysicalSize;
use servo::{
    DeviceIntPoint, DeviceIntRect, DevicePoint, InputEvent, MouseButton, MouseButtonAction,
    MouseButtonEvent, MouseMoveEvent, RenderingContext, Servo, ServoBuilder, SoftwareRenderingContext,
    WebView, WebViewBuilder, WebViewDelegate, WebViewPoint, WheelDelta, WheelEvent, WheelMode,
};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

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

/// Messages from the UI
#[derive(Clone, Debug)]
enum Message {
    UrlChanged(String),
    Go,
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
    servo: Rc<Servo>,
    rendering_context: Rc<SoftwareRenderingContext>,
    webview: WebView,
    frame_handle: Option<Handle>,
    /// Atomic flag set by the delegate when a frame is ready
    frame_ready_flag: Arc<Cell<bool>>,
    /// Current mouse position relative to the webview
    mouse_position: (f32, f32),
}

/// WebViewDelegate that sets an atomic flag when a frame is ready
struct FrameReadyDelegate {
    flag: Arc<Cell<bool>>,
}

impl WebViewDelegate for FrameReadyDelegate {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.flag.set(true);
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

        // Create a WebView with the delegate
        let delegate = Rc::new(FrameReadyDelegate {
            flag: frame_ready_flag.clone(),
        });
        let webview = WebViewBuilder::new(&servo, rendering_context_rc.clone())
            .delegate(delegate)
            .build();

        // Load the initial URL
        let initial_url = "https://github.com/nexoscp/";
        if let Ok(u) = url::Url::parse(initial_url) {
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
                servo: servo_rc,
                rendering_context: rendering_context_rc,
                webview,
                frame_handle,
                frame_ready_flag,
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
                if let Ok(u) = url::Url::parse(&url_str) {
                    self.webview.load(u);
                }
                // Spin once to start processing
                self.servo.spin_event_loop();
                app::Task::none()
            }
            Message::Tick => {
                // Spin Servo's event loop
                self.servo.spin_event_loop();

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
        let button = widget::button::text("Go").on_press(Message::Go);

        let url_input = text_input("https://", &self.url)
            .on_input(Message::UrlChanged)
            .on_submit(|_| Message::Go);
        let url_bar = row![url_input, button,]
            .padding(8)
            .spacing(8);

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

        column![url_bar, webview_content]
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
