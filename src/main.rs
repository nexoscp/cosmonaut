mod webview;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use cosmic::app::{self, Core, Settings};
use cosmic::iced::event;
use cosmic::iced::window;
use cosmic::iced::Length;
use cosmic::iced::Subscription;
use cosmic::iced::widget::column;
use cosmic::iced::widget::progress_bar;
use cosmic::prelude::*;
use cosmic::widget::menu::{bar, items, root, Action, Item, KeyBind, Tree};
use cosmic::widget::row;
use cosmic::widget::segmented_button::{self, SecondaryMap};
use cosmic::widget::tab_bar;
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
    NewTab,
    Exit,
    /// Tab activated (clicked)
    TabActivated(segmented_button::Entity),
    /// Tab reorder from drag-and-drop
    TabReorder(segmented_button::ReorderEvent),
    /// Periodic tick to spin all webview event loops
    Tick,
    /// Webview widget messages (tagged with tab entity for per-tab webviews)
    /// (Used for mouse/scroll events from the widget, not from subscriptions)
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
}

impl From<WebViewMessage> for Message {
    fn from(msg: WebViewMessage) -> Self {
        // This is used by the WebViewWidget; the tab_id is added by the subscription
        // Using a placeholder entity since the real tab_id comes from the subscription
        Message::WebView(segmented_button::Entity::default(), msg)
    }
}

/// Menu actions for the menubar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MenuActionKind {
    NewTab,
    Exit,
}

impl Action for MenuActionKind {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuActionKind::NewTab => Message::NewTab,
            MenuActionKind::Exit => Message::Exit,
        }
    }
}

/// Manages browser navigation history.
#[derive(Clone, Default)]
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
#[derive(Clone)]
struct BrowserState {
    url: String,
    title: Option<String>,
    load_status: LoadStatus,
    status_text: Option<String>,
}

/// Per-tab state containing browser state and history.
#[derive(Clone)]
struct TabState {
    browser: BrowserState,
    history: History,
}

impl TabState {
    fn new(url: impl Into<String>) -> Self {
        Self {
            browser: BrowserState {
                url: url.into(),
                title: None,
                load_status: LoadStatus::Complete,
                status_text: None,
            },
            history: History::default(),
        }
    }

    fn new_tab() -> Self {
        Self {
            browser: BrowserState {
                url: String::new(),
                title: None,
                load_status: LoadStatus::Complete,
                status_text: None,
            },
            history: History::default(),
        }
    }
}

/// State for a single window, containing tabs and their webviews
struct WindowState {
    /// Shared Servo engine for all tabs in this window
    servo: Rc<servo::Servo>,
    /// Tab bar model for managing tabs
    tab_model: segmented_button::SingleSelectModel,
    /// The entity key for the current tab
    tab_id: segmented_button::Entity,
    /// Per-tab state storage (browser state + history)
    tab_state: SecondaryMap<TabState>,
    /// Per-tab webview state storage
    tab_webview: SecondaryMap<Rc<RefCell<webview::WebViewState>>>,
}

impl WindowState {
    fn new(initial_url: &str) -> Self {
        let mut tab_model = segmented_button::SingleSelectModel::default();
        let mut tab_state = SecondaryMap::new();
        let mut tab_webview = SecondaryMap::new();
        
        let tab_id = tab_model.insert().text("New Tab").id();
        tab_model.activate(tab_id);
        tab_state.insert(tab_id, TabState::new(initial_url));
        
        // Create the first webview, which initializes the Servo engine
        let (servo, webview_state) = webview::Builder::default()
            .initial_url(initial_url)
            .transparent_background(true)
            .size(800, 600)
            .build();
        tab_webview.insert(tab_id, Rc::new(RefCell::new(webview_state)));
        
        Self {
            servo,
            tab_model,
            tab_id,
            tab_state,
            tab_webview,
        }
    }
    
    fn create_new_tab(&mut self) -> segmented_button::Entity {
        let new_tab_id = self.tab_model.insert().text("New Tab").id();
        self.tab_model.activate(new_tab_id);
        self.tab_state.insert(new_tab_id, TabState::new_tab());
        
        // Create per-tab webview sharing the same Servo engine
        let webview_state = webview::Builder::default()
            .transparent_background(true)
            .size(800, 600)
            .build_with_servo(Rc::clone(&self.servo));
        self.tab_webview.insert(new_tab_id, Rc::new(RefCell::new(webview_state)));
        
        new_tab_id
    }
    
    fn get_current_webview(&self) -> Option<Rc<RefCell<webview::WebViewState>>> {
        self.tab_webview.get(self.tab_id).cloned()
    }
    
    /// Extract a tab's state and webview for popout to a new window
    fn extract_tab(&mut self, tab_id: segmented_button::Entity) -> Option<(TabState, Rc<RefCell<webview::WebViewState>>)> {
        let tab_state = self.tab_state.remove(tab_id)?;
        let webview_state = self.tab_webview.remove(tab_id)?;
        self.tab_model.remove(tab_id);
        
        // If the popped tab was active, activate another tab
        if self.tab_id == tab_id {
            let new_tab = self.tab_model.iter().next();
            if let Some(new_tab) = new_tab {
                self.tab_id = new_tab;
                self.tab_model.activate(new_tab);
            }
        }
        
        Some((tab_state, webview_state))
    }
}

struct App {
    core: Core,
    /// Multi-window state: maps window ID to its window state
    windows: HashMap<window::Id, WindowState>,
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

        (
            Self {
                core,
                windows,
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
                if let Some(win) = self.get_focused_window() {
                    if let Some(tab) = win.tab_state.get_mut(win.tab_id) {
                        tab.browser.url = u;
                    }
                }
                app::Task::none()
            }
            Message::Go => {
                if let Some(win) = self.get_focused_window() {
                    if let Some(tab) = win.tab_state.get(win.tab_id) {
                        if let Some(webview) = win.get_current_webview() {
                            webview::load_url(&mut webview.borrow_mut(), &tab.browser.url);
                        }
                    }
                }
                app::Task::none()
            }
            Message::Back => {
                if let Some(win) = self.get_focused_window() {
                    if let Some(tab) = win.tab_state.get(win.tab_id) {
                        if tab.history.can_go_back() {
                            if let Some(webview) = win.get_current_webview() {
                                webview::go_back(&mut webview.borrow_mut(), 1);
                            }
                        }
                    }
                }
                app::Task::none()
            }
            Message::Forward => {
                if let Some(win) = self.get_focused_window() {
                    if let Some(tab) = win.tab_state.get(win.tab_id) {
                        if tab.history.can_go_forward() {
                            if let Some(webview) = win.get_current_webview() {
                                webview::go_forward(&mut webview.borrow_mut(), 1);
                            }
                        }
                    }
                }
                app::Task::none()
            }
            Message::NewTab => {
                if let Some(win) = self.get_focused_window() {
                    let new_tab_id = win.create_new_tab();
                    win.tab_id = new_tab_id;
                }
                app::Task::none()
            }
            Message::TabActivated(tab_id) => {
                if let Some(win) = self.get_focused_window() {
                    win.tab_id = tab_id;
                    win.tab_model.activate(tab_id);
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
                // Process tick for all webviews in all windows
                for (_win_id, win) in self.windows.iter_mut() {
                    for (tab_id, webview_state) in win.tab_webview.iter() {
                        let events = webview::update(&mut webview_state.borrow_mut(), WebViewMessage::Tick);
                        for evt in events {
                            match evt {
                                ServoEvent::UrlChanged(url) => {
                                    if let Some(tab) = win.tab_state.get_mut(tab_id) {
                                        tab.browser.url = url.to_string();
                                    }
                                }
                                ServoEvent::TitleChanged(title) => {
                                    if let Some(tab) = win.tab_state.get_mut(tab_id) {
                                        tab.browser.title = title.clone();
                                    }
                                    let tab_title = title.unwrap_or_else(|| "New Tab".to_string());
                                    win.tab_model.text_set(tab_id, tab_title);
                                }
                                ServoEvent::LoadStatusChanged(status) => {
                                    if let Some(tab) = win.tab_state.get_mut(tab_id) {
                                        tab.browser.load_status = status;
                                    }
                                }
                                ServoEvent::StatusTextChanged(status) => {
                                    if let Some(tab) = win.tab_state.get_mut(tab_id) {
                                        tab.browser.status_text = status;
                                    }
                                }
                                ServoEvent::HistoryChanged(entries, current) => {
                                    if let Some(tab) = win.tab_state.get_mut(tab_id) {
                                        tab.history.entries = entries;
                                        tab.history.current = current;
                                    }
                                }
                                ServoEvent::Crashed(reason, _backtrace) => {
                                    eprintln!("Servo crashed: {}", reason);
                                }
                                ServoEvent::CursorChanged(_) | ServoEvent::FrameReady | ServoEvent::FullscreenChanged(_) => {}
                            }
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
                    if let Some((tab_state, webview_state)) = source_win.extract_tab(tab_id) {
                        // Share the Servo engine from the source window
                        let servo = Rc::clone(&source_win.servo);
                        // Create window state with the extracted tab
                        let mut new_win = WindowState {
                            servo,
                            tab_model: segmented_button::SingleSelectModel::default(),
                            tab_id,
                            tab_state: SecondaryMap::new(),
                            tab_webview: SecondaryMap::new(),
                        };
                        let _ = new_win.tab_model.insert().text(
                            tab_state.browser.title.clone().unwrap_or_else(|| "New Tab".to_string())
                        ).id();
                        new_win.tab_model.activate(tab_id);
                        new_win.tab_state.insert(tab_id, tab_state);
                        new_win.tab_webview.insert(tab_id, webview_state);
                        
                        // window::open returns (temporary_id, Task<real_id>)
                        // We store the window state with the temporary ID, then use the Task
                        // to move it to the real ID when it resolves.
                        let (temp_id, open_task) = window::open(window::Settings {
                            position: Default::default(),
                            exit_on_close_request: false,
                            decorations: false,
                            ..Default::default()
                        });
                        
                        // Store with temporary ID; the open task will move it to the real ID
                        self.windows.insert(temp_id, new_win);
                        
                        // Map the resolved real ID to a WindowMoved message, wrapped in cosmic::Action::App
                        open_task.map(move |real_id| cosmic::Action::App(Message::WindowMoved(temp_id, real_id)))
                    } else {
                        app::Task::none()
                    }
                } else {
                    app::Task::none()
                }
            }
        }
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        let win = self.windows.get(&id).expect("window exists");
        let is_main = id == self.core.main_window_id().unwrap_or(window::Id::RESERVED);
        
        // Menu bar
        let key_binds: HashMap<KeyBind, MenuActionKind> = HashMap::new();
        let file_menu = items(&key_binds, vec![
            Item::Button("New Tab", None, MenuActionKind::NewTab),
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

        // Get current tab's state
        let current_tab = win.tab_state.get(win.tab_id).expect("current tab exists");

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
            .on_press_maybe(current_tab.history.can_go_back().then_some(Message::Back))
            .padding(4);
        let forward_button = widget::button::text("→")
            .on_press_maybe(current_tab.history.can_go_forward().then_some(Message::Forward))
            .padding(4);

        // URL bar with adaptive placeholder
        let placeholder = if current_tab.browser.url.starts_with("http://") {
            "http://"
        } else {
            "https://"
        };
        let url_input = text_input(placeholder, &current_tab.browser.url)
            .on_input(Message::UrlChanged)
            .on_submit(|_| Message::Go);
        let go_button = widget::button::text("Go").on_press(Message::Go);

        let url_bar = row![back_button, forward_button, url_input, go_button,]
            .padding(8)
            .spacing(8);

        // Progress bar - hidden when loading is complete
        let progress: Element<'_, Message> = match &current_tab.browser.load_status {
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

        // Webview content - per-tab webview
        let webview_content: Element<'_, Message> = if let Some(webview_state) = win.get_current_webview() {
            let webview_widget = webview::WebViewWidget::new(Rc::clone(&webview_state));
            widget::container(webview_widget)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        } else {
            // Fallback for empty tabs - should not happen in normal operation
            widget::container(text("Empty tab"))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        };

        // Status bar
        let status_text = current_tab.browser.status_text.as_deref().unwrap_or("");
        let status_bar = row![text(status_text)]
            .padding(4)
            .align_y(cosmic::iced::Alignment::Center);

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
            let header = cosmic::widget::header_bar()
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
