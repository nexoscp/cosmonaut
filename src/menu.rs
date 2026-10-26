use crate::message::Message;
use cosmic::widget::menu::action::MenuAction;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuActionKind {
    NewTab,
    About,
    Exit,
}

impl MenuAction for MenuActionKind {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuActionKind::NewTab => Message::NewTab,
            MenuActionKind::About => Message::ContextDrawerToggled,
            MenuActionKind::Exit => Message::Exit,
        }
    }
}