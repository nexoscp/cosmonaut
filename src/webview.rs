//! Servo webview widget for libcosmic applications.
//!
//! This module provides a self-contained widget that wraps the Servo browser engine
//! and integrates it with libcosmic/iced's event loop by implementing the iced `Widget` trait.

use cosmic::iced::advanced::widget::Widget;
use cosmic::iced::advanced::widget::{Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell};
use cosmic::iced::core::image;
use cosmic::iced::core::layout;
use cosmic::iced::core::mouse;
use cosmic::iced::core::mouse::ScrollDelta;
use cosmic::iced::core::renderer;
use cosmic::iced::touch;
use cosmic::iced::widget::image::Handle;
use cosmic::iced::{
    Element, Event, Length, Rectangle, Radians, Size, Subscription,
};
use cosmic::iced::time::{Duration, every};
use cosmic::iced;
use dpi::PhysicalSize;
use servo::{
    Cursor, DeviceIntPoint, DeviceIntRect, DevicePoint, InputEvent, LoadStatus, MouseButton,
    MouseButtonAction, MouseButtonEvent, MouseMoveEvent, Preferences, RenderingContext, Servo,
    ServoBuilder, SoftwareRenderingContext, WebView, WebViewDelegate, WebViewPoint,
    WheelDelta, WheelEvent, WheelMode,
};
use servo::WebViewBuilder as ServoWebViewBuilder;
use std::cell::RefCell;
use std::rc::Rc;
use url::Url;

/// Events emitted by the Servo delegate callbacks.
#[derive(Clone, Debug)]
pub enum ServoEvent {
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

/// Messages that the webview widget emits to the application.
#[derive(Clone, Debug)]
pub enum WebViewMessage {
    /// Periodic tick to spin Servo's event loop and process events
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

/// State required to manage a Servo webview instance.
pub struct WebViewState {
    pub servo: Rc<Servo>,
    pub rendering_context: Rc<SoftwareRenderingContext>,
    pub webview: WebView,
    pub frame_handle: Option<Handle>,
    pub servo_event_receiver: iced::futures::channel::mpsc::UnboundedReceiver<ServoEvent>,
    pub mouse_position: (f32, f32),
}

/// WebViewDelegate that sends events through an iced channel.
struct ServoDelegate {
    sender: Rc<RefCell<iced::futures::channel::mpsc::UnboundedSender<ServoEvent>>>,
}

impl WebViewDelegate for ServoDelegate {
    fn notify_new_frame_ready(&self, _webview: WebView) {
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

/// Local state stored in the widget's Tree.
struct State {
    is_hovered: bool,
    bounds: Rectangle,
}

impl Default for State {
    fn default() -> Self {
        Self {
            is_hovered: false,
            bounds: Rectangle::default(),
        }
    }
}

/// A widget that displays a Servo webview.
pub struct WebViewWidget<'a, Message> {
    state: Rc<RefCell<WebViewState>>,
    _phantom: std::marker::PhantomData<&'a Message>,
}

impl<'a, Message> WebViewWidget<'a, Message> {
    /// Creates a new webview widget from shared state.
    pub fn new(state: Rc<RefCell<WebViewState>>) -> Self {
        Self {
            state,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for WebViewWidget<'_, Message>
where
    Message: Clone + From<WebViewMessage>,
    Renderer: renderer::Renderer + image::Renderer<Handle = Handle>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.width(Length::Fill).height(Length::Fill).max();
        layout::Node::new(size)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let widget_state: &mut State = tree.state.downcast_mut();
        let bounds = layout.bounds();

        // Track hover state
        if let Event::Mouse(mouse::Event::CursorMoved { .. })
        | Event::Touch(touch::Event::FingerMoved { .. })
        | Event::Mouse(mouse::Event::CursorLeft)
        | Event::Touch(touch::Event::FingerLost { .. }) = event
        {
            let _was_hovered = widget_state.is_hovered;
            widget_state.is_hovered = cursor.is_over(bounds);
            widget_state.bounds = bounds;

            // Update mouse position if hovering
            if widget_state.is_hovered {
                if let Some(pos) = cursor.position_in(bounds) {
                    self.state.borrow_mut().mouse_position = (pos.x, pos.y);
                    shell.publish(Message::from(WebViewMessage::MouseMoved(pos.x, pos.y)));
                }
            }
        }

        // Ignore events outside the widget bounds
        if !cursor.is_over(bounds) {
            return;
        }

        // Handle mouse button press
        if let Event::Mouse(mouse::Event::ButtonPressed(button)) = event {
            let servo_button = match button {
                mouse::Button::Left => MouseButton::Primary,
                mouse::Button::Right => MouseButton::Secondary,
                mouse::Button::Middle => MouseButton::Auxiliary,
                _ => return,
            };
            shell.publish(Message::from(WebViewMessage::MousePressed(servo_button)));
            shell.capture_event();
        }

        // Handle touch press
        if let Event::Touch(touch::Event::FingerPressed { .. }) = event {
            shell.publish(Message::from(WebViewMessage::MousePressed(MouseButton::Primary)));
            shell.capture_event();
        }

        // Handle mouse button release
        if let Event::Mouse(mouse::Event::ButtonReleased(button)) = event {
            let servo_button = match button {
                mouse::Button::Left => MouseButton::Primary,
                mouse::Button::Right => MouseButton::Secondary,
                mouse::Button::Middle => MouseButton::Auxiliary,
                _ => return,
            };
            shell.publish(Message::from(WebViewMessage::MouseReleased(servo_button)));
        }

        // Handle touch release
        if let Event::Touch(touch::Event::FingerLifted { .. }) = event {
            shell.publish(Message::from(WebViewMessage::MouseReleased(MouseButton::Primary)));
        }

        // Handle wheel scroll
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
            let (dx, dy) = match delta {
                ScrollDelta::Lines { x, y } => (*x as f64 * 76.0, *y as f64 * 76.0),
                ScrollDelta::Pixels { x, y } => (*x as f64, *y as f64),
            };
            shell.publish(Message::from(WebViewMessage::Scrolled(dx, dy)));
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        if let Some(handle) = &self.state.borrow().frame_handle {
            let drawing_bounds = bounds;

            renderer.draw_image(
                image::Image {
                    handle: handle.clone(),
                    border_radius: [0.0; 4].into(),
                    filter_method: image::FilterMethod::Nearest,
                    rotation: Radians(0.0),
                    opacity: 1.0,
                    snap: true,
                },
                drawing_bounds,
                bounds,
            );
        }
    }
}

impl<'a, Message, Theme, Renderer> From<WebViewWidget<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone + From<WebViewMessage>,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer + image::Renderer<Handle = Handle>,
{
    fn from(widget: WebViewWidget<'a, Message>) -> Element<'a, Message, Theme, Renderer> {
        Element::new(widget)
    }
}

/// Configuration for creating a new webview.
pub struct Builder {
    pub initial_url: Option<String>,
    pub transparent_background: bool,
    pub width: u32,
    pub height: u32,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            initial_url: None,
            transparent_background: false,
            width: 800,
            height: 600,
        }
    }
}

impl Builder {
    /// Set the initial URL to load.
    pub fn initial_url(mut self, url: impl Into<String>) -> Self {
        self.initial_url = Some(url.into());
        self
    }

    /// Enable transparent background for frosted glass effect.
    pub fn transparent_background(mut self, transparent: bool) -> Self {
        self.transparent_background = transparent;
        self
    }

    /// Set the initial rendering size.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Build the Servo engine and webview state.
    pub fn build(self) -> WebViewState {
        let mut preferences = Preferences::default();
        if self.transparent_background {
            preferences.shell_background_color_rgba = [0.0, 0.0, 0.0, 0.0];
        }

        let servo = ServoBuilder::default().preferences(preferences).build();
        servo.setup_logging();

        let initial_size = PhysicalSize {
            width: self.width,
            height: self.height,
        };
        let rendering_context = SoftwareRenderingContext::new(initial_size)
            .expect("Failed to create SoftwareRenderingContext");
        let rendering_context_rc = Rc::new(rendering_context);

        let (servo_event_sender, servo_event_receiver) =
            iced::futures::channel::mpsc::unbounded::<ServoEvent>();

        let delegate = Rc::new(ServoDelegate {
            sender: Rc::new(RefCell::new(servo_event_sender)),
        });
        let webview = ServoWebViewBuilder::new(&servo, rendering_context_rc.clone())
            .delegate(delegate)
            .build();

        // Load initial URL if provided
        if let Some(initial_url) = &self.initial_url {
            if let Ok(u) = Url::parse(initial_url) {
                webview.load(u);
            }
        }

        let servo_rc = Rc::new(servo);
        servo_rc.spin_event_loop();

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
                frame_handle = Some(Handle::from_rgba(
                    size.width,
                    size.height,
                    img.as_raw().clone(),
                ));
            }
        }

        WebViewState {
            servo: servo_rc,
            rendering_context: rendering_context_rc,
            webview,
            frame_handle,
            servo_event_receiver,
            mouse_position: (0.0, 0.0),
        }
    }
}

/// Create a subscription that periodically spins Servo's event loop.
pub fn subscription(_state: &WebViewState) -> Subscription<WebViewMessage> {
    every(Duration::from_millis(16)).map(|_| WebViewMessage::Tick)
}

/// Process a WebViewMessage and update the webview state.
/// Returns a vector of ServoEvent messages that the application should handle.
pub fn update(
    state: &mut WebViewState,
    message: WebViewMessage,
) -> Vec<ServoEvent> {
    let mut events = Vec::new();

    match message {
        WebViewMessage::Tick => {
            // Spin Servo's event loop
            state.servo.spin_event_loop();

            // Process any pending Servo events from the channel
            while let Ok(event) = state.servo_event_receiver.try_recv() {
                events.push(event);
            }

            // Always paint and capture the frame
            state.webview.paint();
            state.rendering_context.present();
            let size = state.rendering_context.size();
            let rect = DeviceIntRect::new(
                DeviceIntPoint::origin(),
                DeviceIntPoint::new(size.width as i32, size.height as i32),
            );
            if let Some(img) = state.rendering_context.read_to_image(rect) {
                state.frame_handle =
                    Some(Handle::from_rgba(size.width, size.height, img.as_raw().clone()));
            }
        }
        WebViewMessage::MousePressed(button) => {
            let (x, y) = state.mouse_position;
            let point = WebViewPoint::Device(DevicePoint::new(x, y));
            let event = InputEvent::MouseButton(MouseButtonEvent::new(
                MouseButtonAction::Down,
                button,
                point,
            ));
            state.webview.notify_input_event(event);
            state.servo.spin_event_loop();
        }
        WebViewMessage::MouseReleased(button) => {
            let (x, y) = state.mouse_position;
            let point = WebViewPoint::Device(DevicePoint::new(x, y));
            let event = InputEvent::MouseButton(MouseButtonEvent::new(
                MouseButtonAction::Up,
                button,
                point,
            ));
            state.webview.notify_input_event(event);
            state.servo.spin_event_loop();
        }
        WebViewMessage::MouseMoved(x, y) => {
            state.mouse_position = (x, y);
            let point = WebViewPoint::Device(DevicePoint::new(x, y));
            let event = InputEvent::MouseMove(MouseMoveEvent::new(point));
            state.webview.notify_input_event(event);
        }
        WebViewMessage::Scrolled(dx, dy) => {
            let (x, y) = state.mouse_position;
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
            state.webview.notify_input_event(event);
            state.servo.spin_event_loop();
        }
    }

    events
}

/// Navigate to a URL.
pub fn load_url(state: &mut WebViewState, url: &str) {
    let url_str = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    };
    if let Ok(u) = Url::parse(&url_str) {
        state.webview.load(u);
    }
    state.servo.spin_event_loop();
}

/// Navigate back in history.
pub fn go_back(state: &mut WebViewState, steps: usize) {
    state.webview.go_back(steps);
    state.servo.spin_event_loop();
}

/// Navigate forward in history.
pub fn go_forward(state: &mut WebViewState, steps: usize) {
    state.webview.go_forward(steps);
    state.servo.spin_event_loop();
}
