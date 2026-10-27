use cargo_toml::Manifest;

fn main() {
    let manifest = Manifest::from_path("Cargo.toml").expect("Cargo.toml");
    let package = manifest.package.expect("package section in Cargo.toml");
    let license = package.license().expect("license");
    println!("cargo:rustc-env=CARGO_PKG_LICENSE={}", license);
    let repository = package.repository().expect("repository");
    println!("cargo:rustc-env=CARGO_PKG_REPOSITORY={}", repository);
}
