//! Window state management for the browser.

use cosmic::iced::window as iced_window;
use cosmic::widget::segmented_button;
use servo::LoadStatus;
use std::collections::HashMap;
use std::rc::Rc;

use crate::tab::TabManager;
use crate::webview;
use crate::webview::{ServoEvent, WebViewMessage, WebViewState};

/// State for a single browser window.
pub struct WindowState {
    /// Shared Servo engine (used to create new tabs)
    pub servo: Rc<servo::Servo>,
    /// Tab manager for this window
    pub tabs: TabManager,
    /// Current URL displayed in the address bar (active tab only)
    pub url: String,
    /// Load status of the active tab (for progress bar)
    pub load_status: LoadStatus,
    /// Status text of the active tab (for status bar)
    pub status_text: Option<String>,
}

impl WindowState {
    /// Create a new window state with an initial URL.
    pub fn new(initial_url: &str) -> Self {
        let (
            servo,
            rendering_context,
            webview_instance,
            servo_event_receiver,
        ) = webview::Builder::default()
            .initial_url(initial_url)
            .transparent_background(true)
            .size(800, 600)
            .build();

        let servo_rc = servo; // build() already returns Rc<Servo>

        // Create the tab manager with the initial tab
        let mut tabs = TabManager::empty();
        tabs.insert_initial_tab(
            "New Tab".to_string(),
            Rc::clone(&servo_rc),
            rendering_context,
            webview_instance,
            servo_event_receiver,
        );

        Self {
            servo: servo_rc,
            tabs,
            url: initial_url.to_string(),
            load_status: LoadStatus::Complete,
            status_text: None,
        }
    }

    /// Get the active tab's webview state.
    pub fn active_webview(&self) -> Option<std::rc::Rc<std::cell::RefCell<WebViewState>>> {
        self.tabs.active_webview()
    }

    /// Navigate to a URL.
    pub fn navigate(&mut self, url: &str) {
        if let Some(webview_state) = self.active_webview() {
            let servo = {
                let state_ref = webview_state.borrow();
                Rc::clone(&state_ref.servo)
            };
            let mut state = webview_state.borrow_mut();
            webview::load_url(&mut state.webview, &servo, url);
        }
    }

    /// Navigate back in history.
    pub fn go_back(&mut self) {
        if let Some(webview_state) = self.active_webview() {
            let servo = {
                let state_ref = webview_state.borrow();
                Rc::clone(&state_ref.servo)
            };
            let mut state = webview_state.borrow_mut();
            let webview = &state.webview;
            if webview.can_go_back() {
                let webview = &mut state.webview;
                webview.go_back(1);
                servo.spin_event_loop();
            }
        }
    }

    /// Navigate forward in history.
    pub fn go_forward(&mut self) {
        if let Some(webview_state) = self.active_webview() {
            let servo = {
                let state_ref = webview_state.borrow();
                Rc::clone(&state_ref.servo)
            };
            let mut state = webview_state.borrow_mut();
            let webview = &state.webview;
            if webview.can_go_forward() {
                let webview = &mut state.webview;
                webview.go_forward(1);
                servo.spin_event_loop();
            }
        }
    }

    /// Check if back navigation is available.
    pub fn can_go_back(&self) -> bool {
        self.tabs.can_go_back()
    }

    /// Check if forward navigation is available.
    pub fn can_go_forward(&self) -> bool {
        self.tabs.can_go_forward()
    }

    /// Create a new tab with its own webview.
    pub fn create_new_tab(&mut self) {
        self.tabs.create_new_tab(&self.servo);
        // Reset state for the new tab
        self.url = webview::current_url(&self.tabs.active_webview().unwrap().borrow().webview);
        self.load_status = LoadStatus::Complete;
        self.status_text = None;
    }

    /// Extract a tab for popout to a new window.
    /// Returns a new WindowState containing the extracted tab.
    pub fn extract_tab_to_window(&mut self, tab_id: segmented_button::Entity) -> Option<WindowState> {
        let tab_title = self.tabs.model().text(tab_id)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "New Tab".to_string());

        let extracted = self.tabs.extract_tab(tab_id)?;
        let url = webview::current_url(&extracted.borrow().webview);

        // Build a new tab manager with the extracted tab
        let mut new_tabs = TabManager::empty();
        let new_tab_id = new_tabs.insert_existing_tab(tab_title, extracted);
        new_tabs.activate(new_tab_id);

        Some(WindowState {
            servo: Rc::clone(&self.servo),
            tabs: new_tabs,
            url,
            load_status: LoadStatus::Complete,
            status_text: None,
        })
    }

    /// Process a webview update and return Servo events.
    pub fn update_webview(&mut self, message: WebViewMessage) -> Vec<ServoEvent> {
        webview::update(&mut self.tabs, message)
    }

    /// Handle a Servo event and update window state.
    pub fn handle_servo_event(&mut self, event: ServoEvent) {
        match event {
            ServoEvent::TitleChanged(title) => {
                let tab_title = title.unwrap_or_else(|| "New Tab".to_string());
                let active_id = self.tabs.active_id();
                self.tabs.model_mut().text_set(active_id, tab_title);
            }
            ServoEvent::LoadStatusChanged(status) => {
                self.load_status = status;
            }
            ServoEvent::StatusTextChanged(status) => {
                self.status_text = status;
            }
            ServoEvent::UrlChanged(url) => {
                self.url = url.to_string();
            }
            ServoEvent::Crashed(reason, _backtrace) => {
                eprintln!("Servo crashed: {}", reason);
            }
            ServoEvent::FrameReady | ServoEvent::CursorChanged(_) | ServoEvent::FullscreenChanged(_) => {
                // No action needed
            }
        }
    }
}

/// Manages all windows in the application.
pub struct WindowManager {
    /// Multi-window state: maps window ID to its window state
    windows: HashMap<iced_window::Id, WindowState>,
}

impl WindowManager {
    /// Create a new window manager with the main window.
    pub fn new(main_window_id: iced_window::Id, initial_url: &str) -> Self {
        let mut windows = HashMap::new();
        windows.insert(main_window_id, WindowState::new(initial_url));
        Self { windows }
    }

    /// Get a mutable reference to the focused window's state.
    pub fn get_focused_window(&mut self, focused_id: iced_window::Id) -> Option<&mut WindowState> {
        self.windows.get_mut(&focused_id)
    }

    /// Get a reference to a specific window's state.
    pub fn get_window(&self, id: iced_window::Id) -> Option<&WindowState> {
        self.windows.get(&id)
    }

    /// Get a mutable reference to a specific window's state.
    pub fn get_window_mut(&mut self, id: iced_window::Id) -> Option<&mut WindowState> {
        self.windows.get_mut(&id)
    }

    /// Add a new window.
    pub fn add_window(&mut self, id: iced_window::Id, state: WindowState) {
        self.windows.insert(id, state);
    }

    /// Remove a window.
    pub fn remove_window(&mut self, id: iced_window::Id) {
        self.windows.remove(&id);
    }

    /// Move a window from one ID to another.
    pub fn move_window(&mut self, from: iced_window::Id, to: iced_window::Id) {
        if let Some(state) = self.windows.remove(&from) {
            self.windows.insert(to, state);
        }
    }

    /// Iterate over all windows.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&iced_window::Id, &mut WindowState)> {
        self.windows.iter_mut()
    }

    /// Check if a window exists.
    pub fn contains(&self, id: iced_window::Id) -> bool {
        self.windows.contains_key(&id)
    }
}
