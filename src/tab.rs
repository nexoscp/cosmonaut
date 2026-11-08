//! Tab management logic for the browser.

use cosmic::widget::segmented_button;
use std::cell::RefCell;
use std::rc::Rc;

use crate::webview;
use crate::webview::{Builder, WebView, WebViewState};
use crate::webview::RenderingContext;
use cosmic::iced::futures::channel::mpsc::UnboundedReceiver;
use crate::webview::ServoEvent;

/// Manages tabs within a window.
pub struct TabManager {
    /// Tab bar model for managing tabs
    model: segmented_button::SingleSelectModel,
    /// The entity key for the current active tab
    active_id: segmented_button::Entity,
    /// Per-tab webview states
    tab_webviews: segmented_button::SecondaryMap<Rc<RefCell<WebViewState>>>,
}

impl TabManager {
    /// Create an empty tab manager.
    pub fn empty() -> Self {
        let mut model = segmented_button::SingleSelectModel::default();
        // Insert a placeholder so we have a valid active_id
        let active_id = model.insert().text("").id();
        model.activate(active_id);

        Self {
            model,
            active_id,
            tab_webviews: segmented_button::SecondaryMap::new(),
        }
    }

    /// Insert the initial tab with pre-built webview components.
    pub fn insert_initial_tab(
        &mut self,
        title: String,
        servo: Rc<servo::Servo>,
        rendering_context: Rc<dyn RenderingContext>,
        webview: WebView,
        servo_event_receiver: UnboundedReceiver<ServoEvent>,
    ) {
        // Remove the placeholder
        self.model.remove(self.active_id);

        let active_id = self.model.insert().text(title).id();
        self.model.activate(active_id);
        self.active_id = active_id;

        self.tab_webviews.insert(
            active_id,
            Rc::new(RefCell::new(WebViewState {
                servo,
                rendering_context,
                webview,
                frame_handle: None,
                servo_event_receiver,
                mouse_position: (0.0, 0.0),
                frame_dirty: false,
            })),
        );
    }

    /// Insert an existing tab (e.g., from a popout).
    pub fn insert_existing_tab(
        &mut self,
        title: String,
        webview_state: Rc<RefCell<WebViewState>>,
    ) -> segmented_button::Entity {
        let tab_id = self.model.insert().text(title).id();
        self.tab_webviews.insert(tab_id, webview_state);
        tab_id
    }

    /// Get the active tab's entity ID.
    pub fn active_id(&self) -> segmented_button::Entity {
        self.active_id
    }

    /// Get a reference to the tab model.
    pub fn model(&self) -> &segmented_button::SingleSelectModel {
        &self.model
    }

    /// Get a mutable reference to the tab model.
    pub fn model_mut(&mut self) -> &mut segmented_button::SingleSelectModel {
        &mut self.model
    }

    /// Get the active tab's webview state.
    pub fn active_webview(&self) -> Option<Rc<RefCell<WebViewState>>> {
        self.tab_webviews.get(self.active_id).cloned()
    }

    /// Get a specific tab's webview state by entity ID.
    #[allow(dead_code)]
    pub fn get_webview(&self, tab_id: segmented_button::Entity) -> Option<Rc<RefCell<WebViewState>>> {
        self.tab_webviews.get(tab_id).cloned()
    }

    /// Activate a tab by its entity ID.
    pub fn activate(&mut self, tab_id: segmented_button::Entity) {
        self.active_id = tab_id;
        self.model.activate(tab_id);
    }

    /// Create a new tab with its own webview.
    /// Returns the entity ID of the new tab.
    pub fn create_new_tab(&mut self, servo: &Rc<servo::Servo>) -> segmented_button::Entity {
        let new_tab_id = self.model.insert().text("New Tab").id();
        self.model.activate(new_tab_id);

        let (
            _servo,
            rendering_context,
            webview,
            servo_event_receiver,
        ) = Builder::default()
            .transparent_background(true)
            .size(800, 600)
            .build_with_servo(Rc::clone(servo));

        self.tab_webviews.insert(
            new_tab_id,
            Rc::new(RefCell::new(WebViewState {
                servo: Rc::clone(servo),
                rendering_context,
                webview,
                frame_handle: None,
                servo_event_receiver,
                mouse_position: (0.0, 0.0),
                frame_dirty: false,
            })),
        );

        self.active_id = new_tab_id;
        new_tab_id
    }

    /// Remove a tab from the model.
    #[allow(dead_code)]
    pub fn remove_tab(&mut self, tab_id: segmented_button::Entity) {
        self.tab_webviews.remove(tab_id);
        self.model.remove(tab_id);

        // If the removed tab was active, activate another tab
        if self.active_id == tab_id {
            let iter = self.model.iter().collect::<Vec<_>>();
            if let Some(new_tab) = iter.into_iter().next() {
                self.active_id = new_tab;
                self.model.activate(new_tab);
            }
        }
    }

    /// Extract a tab's webview for popout to a new window.
    /// Returns the extracted webview state, or None if the tab doesn't exist.
    pub fn extract_tab(&mut self, tab_id: segmented_button::Entity) -> Option<Rc<RefCell<WebViewState>>> {
        let webview_state = self.tab_webviews.remove(tab_id);
        self.model.remove(tab_id);

        // If the removed tab was active, activate another tab
        if self.active_id == tab_id {
            let iter = self.model.iter().collect::<Vec<_>>();
            if let Some(new_tab) = iter.into_iter().next() {
                self.active_id = new_tab;
                self.model.activate(new_tab);
            }
        }

        webview_state
    }

    /// Reorder tabs based on a drag-and-drop event.
    pub fn reorder(&mut self, event: segmented_button::ReorderEvent) {
        let mut model = std::mem::take(&mut self.model);
        model.reorder(event.dragged, event.target, event.position);
        self.model = model;
    }

    /// Get the URL of the active tab's webview.
    pub fn active_url(&self) -> String {
        if let Some(webview_state) = self.active_webview() {
            webview::current_url(&webview_state.borrow().webview)
        } else {
            String::new()
        }
    }

    /// Check if back navigation is available for the active tab.
    pub fn can_go_back(&self) -> bool {
        if let Some(webview_state) = self.active_webview() {
            let webview = &webview_state.borrow().webview;
            webview.can_go_back()
        } else {
            false
        }
    }

    /// Check if forward navigation is available for the active tab.
    pub fn can_go_forward(&self) -> bool {
        if let Some(webview_state) = self.active_webview() {
            let webview = &webview_state.borrow().webview;
            webview.can_go_forward()
        } else {
            false
        }
    }
}
