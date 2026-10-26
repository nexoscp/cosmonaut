use crate::webview::WebViewMessage;
use cosmic::iced::window;
use cosmic::widget::segmented_button;

/// Messages from the UI
#[derive(Clone, Debug)]
pub enum Message {
    UrlChanged(String),
    Go,
    Back,
    Forward,
    NewTab,
    Exit,
    /// Tab activated (clicked)
    TabActivated(segmented_button::Entity),
    /// Tab reorder from drag-and-drop
    TabReorder(segmented_button::ReorderEvent),
    /// Periodic tick to spin all webview event loops
    Tick,
    /// Webview widget messages (tagged with tab entity for per-tab webviews)
    /// Mouse/scroll events from the widget are handled directly by the widget's on_message callback.
    #[allow(dead_code)]
    WebView(segmented_button::Entity, WebViewMessage),
    /// Window lifecycle events
    WindowOpened(window::Id),
    WindowClosed(window::Id),
    CloseWindow(window::Id),
    /// Tab popped out to new window (triggered when tab is dragged outside the widget)
    #[allow(dead_code)]
    TabPoppedOut(segmented_button::Entity),
    /// Window state needs to move from temporary ID to real ID after window::open resolves
    WindowMoved(window::Id, window::Id),
    /// Toggle the context drawer (About page)
    ContextDrawerToggled,
    /// URL link pressed in the About page
    AboutUrlPressed(String),
}

impl From<WebViewMessage> for Message {
    fn from(msg: WebViewMessage) -> Self {
        // Required by WebViewWidget's type bounds for Element conversion
        Message::WebView(segmented_button::Entity::default(), msg)
    }
}