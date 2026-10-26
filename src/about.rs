use cosmic::widget::about::About;

pub fn about() -> About {
    About::default()
        .name("Cosmonaut")
        .version("0.1.0")
        .author("Nexos")
        .developers(vec![("Nexos", "nexos@nexoscp.org")])
        .license("MIT")
        .license_url("https://opensource.org/licenses/MIT")
        .links(vec![
            ("Website", "https://github.com/nexoscp/cosmonaut"),
            ("Source", "https://github.com/nexoscp/cosmonaut"),
        ])
        .comments("A web browser powered by Servo")
}