//! Servo webview widget for libcosmic applications.
//!
//! This module provides a self-contained widget that wraps the Servo browser engine
//! and integrates it with libcosmic/iced's event loop by implementing the iced `Widget` trait.

pub use servo::RenderingContext;
pub use servo::WebView;

pub use cosmic::widget::segmented_button;

use cosmic::iced;
use cosmic::iced::advanced::widget::Widget;
use cosmic::iced::advanced::widget::{Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell};
use cosmic::iced::core::image;
use cosmic::iced::core::layout;
use cosmic::iced::core::mouse;
use cosmic::iced::core::mouse::ScrollDelta;
use cosmic::iced::core::renderer;
use cosmic::iced::time::{Duration, every};
use cosmic::iced::touch;
use cosmic::iced::widget::image::Handle;
use cosmic::iced::{
    Element, Event, Length, Radians, Rectangle, Size, Subscription,
};
use dpi::PhysicalSize;
use servo::WebViewBuilder as ServoWebViewBuilder;
use servo::{
    Cursor, DeviceIntPoint, DeviceIntRect, DevicePoint, InputEvent, LoadStatus, MouseButton,
    MouseButtonAction, MouseButtonEvent, MouseMoveEvent, Preferences, Servo,
    ServoBuilder, SoftwareRenderingContext, WebViewDelegate, WebViewPoint,
    WheelDelta, WheelEvent, WheelMode,
};
use std::cell::RefCell;
use std::rc::Rc;
use url::Url;

/// Events emitted by the Servo delegate callbacks.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum ServoEvent {
    UrlChanged(Url),
    TitleChanged(Option<String>),
    LoadStatusChanged(LoadStatus),
    StatusTextChanged(Option<String>),
    CursorChanged(Cursor),
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

/// Map an iced mouse button to a Servo MouseButton.
fn to_servo_button(button: mouse::Button) -> Option<MouseButton> {
    match button {
        mouse::Button::Left => Some(MouseButton::Primary),
        mouse::Button::Right => Some(MouseButton::Secondary),
        mouse::Button::Middle => Some(MouseButton::Auxiliary),
        _ => None,
    }
}

/// WebViewDelegate that sends events through an iced channel.
struct ServoDelegate {
    sender: Rc<RefCell<iced::futures::channel::mpsc::UnboundedSender<ServoEvent>>>,
}

impl ServoDelegate {
    fn send(&self, event: ServoEvent) {
        let _ = self.sender.borrow_mut().start_send(event);
    }
}

impl WebViewDelegate for ServoDelegate {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.send(ServoEvent::FrameReady);
    }

    fn notify_url_changed(&self, _webview: WebView, url: Url) {
        self.send(ServoEvent::UrlChanged(url));
    }

    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        self.send(ServoEvent::TitleChanged(title));
    }

    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        self.send(ServoEvent::LoadStatusChanged(status));
    }

    fn notify_status_text_changed(&self, _webview: WebView, status: Option<String>) {
        self.send(ServoEvent::StatusTextChanged(status));
    }

    fn notify_cursor_changed(&self, _webview: WebView, cursor: Cursor) {
        self.send(ServoEvent::CursorChanged(cursor));
    }

    fn notify_history_changed(
        &self,
        _webview: WebView,
        _entries: Vec<Url>,
        _current: usize,
    ) {
        // History is managed by Servo; Cosmonaut queries it directly when needed.
    }

    fn notify_crashed(
        &self,
        _webview: WebView,
        reason: String,
        backtrace: Option<String>,
    ) {
        self.send(ServoEvent::Crashed(reason, backtrace));
    }

    fn notify_fullscreen_state_changed(&self, _webview: WebView, is_fullscreen: bool) {
        self.send(ServoEvent::FullscreenChanged(is_fullscreen));
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

/// A widget that displays a Servo webview for the current tab.
pub(crate) struct WebViewWidget<'a, Message> {
    window: &'a WindowState,
    _phantom: std::marker::PhantomData<&'a Message>,
}

impl<'a, Message> WebViewWidget<'a, Message> {
    /// Creates a new webview widget from a window reference.
    pub(crate) fn new(window: &'a WindowState) -> Self {
        Self {
            window,
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
            widget_state.is_hovered = cursor.is_over(bounds);
            widget_state.bounds = bounds;
        }

        // Ignore events outside the widget bounds
        if !cursor.is_over(bounds) {
            return;
        }

        // Handle mouse button press
        if let Event::Mouse(mouse::Event::ButtonPressed(button)) = event {
            if let Some(servo_button) = to_servo_button(*button) {
                shell.publish(Message::from(WebViewMessage::MousePressed(servo_button)));
                shell.capture_event();
            }
        }

        // Handle touch press
        if let Event::Touch(touch::Event::FingerPressed { .. }) = event {
            shell.publish(Message::from(WebViewMessage::MousePressed(MouseButton::Primary)));
            shell.capture_event();
        }

        // Handle mouse button release
        if let Event::Mouse(mouse::Event::ButtonReleased(button)) = event {
            if let Some(servo_button) = to_servo_button(*button) {
                shell.publish(Message::from(WebViewMessage::MouseReleased(servo_button)));
            }
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

        if let Some(webview_state) = self.window.current_webview() {
            if let Some(handle) = &webview_state.borrow().frame_handle {
                renderer.draw_image(
                    image::Image {
                        handle: handle.clone(),
                        border_radius: [0.0; 4].into(),
                        filter_method: image::FilterMethod::Nearest,
                        rotation: Radians(0.0),
                        opacity: 1.0,
                        snap: true,
                    },
                    bounds,
                    bounds,
                );
            }
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

    /// Build a new Servo engine and webview components.
    /// Returns the components for WindowState.
    pub fn build(self) -> (
        Rc<Servo>,
        Rc<dyn RenderingContext>,
        WebView,
        iced::futures::channel::mpsc::UnboundedReceiver<ServoEvent>,
    ) {
        let mut preferences = Preferences::default();
        if self.transparent_background {
            preferences.shell_background_color_rgba = [0.0, 0.0, 0.0, 0.0];
        }

        let servo = ServoBuilder::default().preferences(preferences).build();
        servo.setup_logging();

        let servo_rc = Rc::new(servo);
        let (rendering_context, webview, receiver) = self.build_components(Rc::clone(&servo_rc));
        (servo_rc, rendering_context, webview, receiver)
    }

    /// Build webview components using an existing shared Servo engine.
    pub fn build_with_servo(self, servo: Rc<Servo>) -> (
        Rc<Servo>,
        Rc<dyn RenderingContext>,
        WebView,
        iced::futures::channel::mpsc::UnboundedReceiver<ServoEvent>,
    ) {
        let (rendering_context, webview, receiver) = self.build_components(Rc::clone(&servo));
        (servo, rendering_context, webview, receiver)
    }

    fn build_components(
        self,
        servo: Rc<Servo>,
    ) -> (
        Rc<dyn RenderingContext>,
        WebView,
        iced::futures::channel::mpsc::UnboundedReceiver<ServoEvent>,
    ) {
        let initial_size = PhysicalSize {
            width: self.width,
            height: self.height,
        };
        let rendering_context = SoftwareRenderingContext::new(initial_size)
            .expect("Failed to create SoftwareRenderingContext");
        let rendering_context_rc: Rc<dyn RenderingContext> = Rc::new(rendering_context);

        let (servo_event_sender, servo_event_receiver) =
            iced::futures::channel::mpsc::unbounded::<ServoEvent>();

        let delegate = Rc::new(ServoDelegate {
            sender: Rc::new(RefCell::new(servo_event_sender)),
        });
        let webview = ServoWebViewBuilder::new(&servo, Rc::clone(&rendering_context_rc))
            .delegate(delegate)
            .build();

        // Load initial URL if provided
        if let Some(initial_url) = &self.initial_url {
            if let Ok(u) = Url::parse(initial_url) {
                webview.load(u);
            }
        }

        servo.spin_event_loop();

        (rendering_context_rc, webview, servo_event_receiver)
    }
}

/// Create a subscription that periodically spins Servo's event loop.
pub fn subscription() -> Subscription<WebViewMessage> {
    every(Duration::from_millis(16)).map(|_| WebViewMessage::Tick)
}

/// Process a WebViewMessage and update the current tab's webview state.
/// Returns a vector of ServoEvent messages that the application should handle.
pub fn update(win: &mut WindowState, message: WebViewMessage) -> Vec<ServoEvent> {
    let webview_state = match win.current_webview() {
        Some(state) => state,
        None => return Vec::new(),
    };

    match message {
        WebViewMessage::Tick => {
            // Clone servo Rc before mutable borrow
            let servo = {
                let state_ref = webview_state.borrow();
                Rc::clone(&state_ref.servo)
            };

            // Spin Servo's event loop
            servo.spin_event_loop();

            // Process any pending Servo events from the channel
            let mut events = Vec::new();
            let mut needs_repaint = false;
            {
                let mut state = webview_state.borrow_mut();
                let servo_event_receiver = &mut state.servo_event_receiver;
                while let Ok(event) = servo_event_receiver.try_recv() {
                    if matches!(event, ServoEvent::FrameReady) {
                        needs_repaint = true;
                    }
                    events.push(event);
                }

                // Only paint and capture the frame when content actually changed
                if needs_repaint || state.frame_dirty {
                    state.webview.paint();
                    state.rendering_context.present();
                    let size = state.rendering_context.size();
                    let rect = DeviceIntRect::new(
                        DeviceIntPoint::origin(),
                        DeviceIntPoint::new(size.width as i32, size.height as i32),
                    );
                    if let Some(img) = state.rendering_context.read_to_image(rect) {
                        state.frame_handle = Some(Handle::from_rgba(size.width, size.height, img.as_raw().clone()));
                    }
                    state.frame_dirty = false;
                }
            }

            events
        }
        WebViewMessage::MousePressed(button) => {
            let servo = {
                let state_ref = webview_state.borrow();
                Rc::clone(&state_ref.servo)
            };
            let mut state = webview_state.borrow_mut();
            let point = WebViewPoint::Device(DevicePoint::new(state.mouse_position.0, state.mouse_position.1));
            let event = InputEvent::MouseButton(MouseButtonEvent::new(
                MouseButtonAction::Down,
                button,
                point,
            ));
            state.webview.notify_input_event(event);
            servo.spin_event_loop();
            state.frame_dirty = true;
            Vec::new()
        }
        WebViewMessage::MouseReleased(button) => {
            let servo = {
                let state_ref = webview_state.borrow();
                Rc::clone(&state_ref.servo)
            };
            let mut state = webview_state.borrow_mut();
            let point = WebViewPoint::Device(DevicePoint::new(state.mouse_position.0, state.mouse_position.1));
            let event = InputEvent::MouseButton(MouseButtonEvent::new(
                MouseButtonAction::Up,
                button,
                point,
            ));
            state.webview.notify_input_event(event);
            servo.spin_event_loop();
            state.frame_dirty = true;
            Vec::new()
        }
        WebViewMessage::MouseMoved(x, y) => {
            let mut state = webview_state.borrow_mut();
            state.mouse_position = (x, y);
            let point = WebViewPoint::Device(DevicePoint::new(x, y));
            let event = InputEvent::MouseMove(MouseMoveEvent::new(point));
            state.webview.notify_input_event(event);
            Vec::new()
        }
        WebViewMessage::Scrolled(dx, dy) => {
            let servo = {
                let state_ref = webview_state.borrow();
                Rc::clone(&state_ref.servo)
            };
            let mut state = webview_state.borrow_mut();
            let point = WebViewPoint::Device(DevicePoint::new(state.mouse_position.0, state.mouse_position.1));
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
            servo.spin_event_loop();
            state.frame_dirty = true;
            Vec::new()
        }
    }
}

/// Navigate to a URL.
pub fn load_url(webview: &mut WebView, servo: &Servo, url: &str) {
    let url_str = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    };
    if let Ok(u) = Url::parse(&url_str) {
        webview.load(u);
    }
    servo.spin_event_loop();
}

/// Navigate back in history.
pub fn go_back(webview: &mut WebView, servo: &Servo, steps: usize) {
    webview.go_back(steps);
    servo.spin_event_loop();
}

/// Navigate forward in history.
pub fn go_forward(webview: &mut WebView, servo: &Servo, steps: usize) {
    webview.go_forward(steps);
    servo.spin_event_loop();
}

/// Check if back navigation is available.
pub fn can_go_back(webview: &WebView) -> bool {
    webview.can_go_back()
}

/// Check if forward navigation is available.
pub fn can_go_forward(webview: &WebView) -> bool {
    webview.can_go_forward()
}

/// Get the current URL of the webview.
pub fn current_url(webview: &WebView) -> String {
    webview.url().map(|u| u.to_string()).unwrap_or_default()
}

/// Per-tab webview state.
pub struct WebViewState {
    /// Shared Servo engine
    pub servo: Rc<Servo>,
    /// Rendering context for the webview
    pub rendering_context: Rc<dyn RenderingContext>,
    /// The Servo WebView instance
    pub webview: WebView,
    /// Captured frame image for display
    pub frame_handle: Option<Handle>,
    /// Channel receiver for Servo events
    pub servo_event_receiver: iced::futures::channel::mpsc::UnboundedReceiver<ServoEvent>,
    /// Current mouse position over the webview
    pub mouse_position: (f32, f32),
    /// Whether the frame needs to be recaptured on the next tick.
    pub frame_dirty: bool,
}

/// State for a single window, containing tabs and per-tab webviews.
pub struct WindowState {
    /// Shared Servo engine (used to create new tabs)
    pub servo: Rc<Servo>,
    /// Per-tab webview states
    pub tab_webview: segmented_button::SecondaryMap<Rc<RefCell<WebViewState>>>,
    /// Current URL displayed in the address bar (active tab only)
    pub url: String,
    /// Tab bar model for managing tabs
    pub tab_model: segmented_button::SingleSelectModel,
    /// The entity key for the current tab
    pub tab_id: segmented_button::Entity,
    /// Load status of the active tab (for progress bar)
    pub load_status: LoadStatus,
    /// Status text of the active tab (for status bar)
    pub status_text: Option<String>,
}

impl WindowState {
    /// Create a new window state with an initial URL.
    pub fn new(initial_url: &str) -> Self {
        let mut tab_model = segmented_button::SingleSelectModel::default();

        let tab_id = tab_model.insert().text("New Tab").id();
        tab_model.activate(tab_id);

        // Create the Servo engine and webview
        let (
            servo,
            rendering_context,
            webview,
            servo_event_receiver,
        ) = Builder::default()
            .initial_url(initial_url)
            .transparent_background(true)
            .size(800, 600)
            .build();

        let mut tab_webview = segmented_button::SecondaryMap::new();
        tab_webview.insert(
            tab_id,
            Rc::new(RefCell::new(WebViewState {
                servo: Rc::clone(&servo),
                rendering_context,
                webview,
                frame_handle: None,
                servo_event_receiver,
                mouse_position: (0.0, 0.0),
                frame_dirty: false,
            })),
        );

        Self {
            servo,
            tab_webview,
            url: initial_url.to_string(),
            tab_model,
            tab_id,
            load_status: LoadStatus::Complete,
            status_text: None,
        }
    }

    /// Get the current tab's webview state.
    pub fn current_webview(&self) -> Option<Rc<RefCell<WebViewState>>> {
        self.tab_webview.get(self.tab_id).cloned()
    }

    /// Create a new tab with its own webview.
    pub fn create_new_tab(&mut self) -> segmented_button::Entity {
        let new_tab_id = self.tab_model.insert().text("New Tab").id();
        self.tab_model.activate(new_tab_id);

        // Create a new webview sharing the Servo engine
        let (
            _servo,
            rendering_context,
            webview,
            servo_event_receiver,
        ) = Builder::default()
            .transparent_background(true)
            .size(800, 600)
            .build_with_servo(Rc::clone(&self.servo));

        // Capture URL before moving webview
        let url = current_url(&webview);

        self.tab_webview.insert(
            new_tab_id,
            Rc::new(RefCell::new(WebViewState {
                servo: Rc::clone(&self.servo),
                rendering_context,
                webview,
                frame_handle: None,
                servo_event_receiver,
                mouse_position: (0.0, 0.0),
                frame_dirty: false,
            })),
        );

        // Reset state for the new tab
        self.url = url;
        self.load_status = LoadStatus::Complete;
        self.status_text = None;
        self.tab_id = new_tab_id;

        new_tab_id
    }

    /// Remove a tab from the model.
    #[allow(dead_code)]
    pub fn remove_tab(&mut self, tab_id: segmented_button::Entity) {
        self.tab_webview.remove(tab_id);
        self.tab_model.remove(tab_id);

        // If the removed tab was active, activate another tab
        if self.tab_id == tab_id {
            let iter = self.tab_model.iter().collect::<Vec<_>>();
            if let Some(new_tab) = iter.into_iter().next() {
                self.tab_id = new_tab;
                self.tab_model.activate(new_tab);
                // Sync URL from the new active tab's webview
                if let Some(webview_state) = self.current_webview() {
                    self.url = current_url(&webview_state.borrow().webview);
                }
            }
        }
    }

    /// Extract a tab's webview for popout to a new window.
    pub fn extract_tab(&mut self, tab_id: segmented_button::Entity) -> Option<Rc<RefCell<WebViewState>>> {
        let webview_state = self.tab_webview.remove(tab_id);
        self.tab_model.remove(tab_id);

        // If the removed tab was active, activate another tab
        if self.tab_id == tab_id {
            let iter = self.tab_model.iter().collect::<Vec<_>>();
            if let Some(new_tab) = iter.into_iter().next() {
                self.tab_id = new_tab;
                self.tab_model.activate(new_tab);
                if let Some(remaining_webview) = self.current_webview() {
                    self.url = current_url(&remaining_webview.borrow().webview);
                }
            }
        }

        webview_state
    }
}
