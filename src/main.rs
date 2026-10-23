use cosmic::app::{self, Core, Settings};
use cosmic::dialog::ashpd::url;
use cosmic::iced::Length;
use cosmic::iced::widget::column;
use cosmic::prelude::*;
use cosmic::widget::row;
use cosmic::widget::text;
use cosmic::widget::text_input;
use cosmic::{executor, iced, widget};
// use servo::{EventLoopWaker
use servo::{RenderingContext, Servo, SoftwareRenderingContext, WebView};
use std::rc::Rc;

//#[derive(Send)]
//struct Waker(app::Task<Message>);

/* impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }

    fn wake(&self) {
        // The task will be sent to the executor in the update loop
    }
} */

#[derive(Clone, Debug)]
enum Message {
    UrlChanged(String),
    Go,
    Resize,
    SpinServo,
}

struct App {
    core: Core,
    url: String,
    servo: Option<Rc<Servo>>,
    rendering_context: Option<Rc<SoftwareRenderingContext>>,
    webview: Option<WebView>,
 //   waker: Option<Waker>,
}

impl cosmic::Application for App {
    type Message = Message;
    type Executor = executor::Default;
    type Flags = ();
    const APP_ID: &'static str = "nexos.Cosmonaut";

    fn init(core: Core, _flags: Self::Flags) -> (Self, app::Task<Message>) {
        (
            Self {
                core,
                url: "https://github.com/nexoscp/cosmonaut".to_string(),
                servo: None,
                rendering_context: None,
                webview: None,
              //  waker: None,
            },
            app::Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::UrlChanged(url) => {
                self.url = url;
                app::Task::none()
            }
            Message::Go => {
                if let Some(ref webview) = self.webview {
                    let url_str = if self.url.starts_with("http://") || self.url.starts_with("https://") {
                        self.url.clone()
                    } else {
                        format!("https://{}", self.url)
                    };
                    if let Ok(url) = url::Url::parse(&url_str) {
                        webview.load(url);
                    }
                }
                app::Task::none()
            }
            Message::Resize => {
                if let Some(ref webview) = self.webview {
                    if let Some(ref rc) = self.rendering_context {
                        let size = rc.size();
                        webview.resize(size);
                    }
                }
                app::Task::none()
            }
            Message::SpinServo => {
                if let Some(ref servo) = self.servo {
                    servo.spin_event_loop();
                }
                app::Task::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let button = widget::button::text("Go")
            .on_press(Message::Go);

        let url_bar = row![
            text_input("https://", &self.url),
            button,
        ]
        .padding(8)
        .spacing(8);

        let webview_placeholder = widget::container(text("Servo webview will appear here"))
            .width(Length::Fill)
            .height(Length::Fill);
            //.center_x(/* width */)
            //.center_y()
            //.into();

        column![url_bar, webview_placeholder]
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
 /*   rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install crypto provider");*/

    app::run::<App>(Settings::default(), ())
}