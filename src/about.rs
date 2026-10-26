use cosmic::widget::about::About;

/// Package name from Cargo.toml
const NAME: &str = env!("CARGO_PKG_NAME");
/// Package version from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Package authors from Cargo.toml (format: "name\temail" per author, separated by newlines)
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
/// Package description from Cargo.toml
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
/// License from Cargo.toml, exposed via build.rs
const LICENSE: &str = env!("CARGO_PKG_LICENSE");
/// Repository URL from Cargo.toml, exposed via build.rs
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Parse the CARGO_PKG_AUTHORS string into a vector of (name, email) pairs.
/// Format is "name\temail" per author, separated by newlines.
/// If no email is present, only the name is used.
fn parse_authors(authors: &str) -> Vec<(&str, Option<&str>)> {
    authors
        .lines()
        .map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            match parts.len() {
                0 => vec![],
                1 => vec![(parts[0], None)],
                _ => vec![(parts[0], Some(parts[1]))],
            }
        })
        .flatten()
        .collect()
}

/// Generate the About page data from Cargo.toml metadata.
pub fn about() -> About {
    let authors = parse_authors(AUTHORS);

    // Build the developers list from authors
    let developers: Vec<(&str, &str)> = authors
        .iter()
        .map(|(name, email)| {
            (
                *name,
                email.unwrap_or_else(|| "unknown@unvalid.org"),
            )
        })
        .collect();

    // Determine the primary author name
    let author_name = authors.first().map(|(name, _)| *name).unwrap_or("Unknown");

    // Map SPDX license identifiers to URLs
    let license_url = match LICENSE {
        "MIT" => "https://opensource.org/licenses/MIT",
        "Apache-2.0" => "https://opensource.org/licenses/Apache-2.0",
        "MPL-2.0" => "https://opensource.org/licenses/MPL-2.0",
        "GPL-2.0" | "GPL-3.0" => "https://www.gnu.org/licenses/",
        _ => "https://opensource.org/",
    };

    About::default()
        .name(NAME)
        .version(VERSION)
        .author(author_name)
        .developers(developers)
        .license(LICENSE)
        .license_url(license_url)
        .links(vec![
            ("Website", REPOSITORY),
            ("Source", REPOSITORY),
        ])
        .comments(DESCRIPTION)
}