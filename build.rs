use std::path::Path;

fn main() {
    let manifest = Path::new("Cargo.toml");
    if let Ok(content) = std::fs::read_to_string(manifest) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("license = ") {
                let license = line.trim_start_matches("license = ").trim_matches('"');
                println!("cargo:rustc-env=CARGO_PKG_LICENSE={}", license);
            } else if line.starts_with("repository = ") {
                let repository = line.trim_start_matches("repository = ").trim_matches('"');
                println!("cargo:rustc-env=CARGO_PKG_REPOSITORY={}", repository);
            }
        }
    }
}
