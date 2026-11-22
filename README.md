# Cosmonaut

[Servo](https://github.com/servo/servo) browser engine in a [libcosmic](https://github.com/pop-os/libcosmic) shell

![Cosmonaut Screenshot](cosmonaut.jpeg)

# Similar Projects

I wasn't the first with the idea to bring [Servo](https://github.com/servo/servo) and [libcosmic](https://github.com/pop-os/libcosmic) together:

 * [cosmic-web](https://codeberg.org/snaggen/cosmic-web)
 * [Astrolabe](https://github.com/snaggen/astrolabe)

## Build

* apt install -y libclang-dev clang
* cargo build

## Versioning

The version in `Cargo.toml` is automatically bumped from git tags during GitHub Releases
using [tj-actions/cargo-bump](https://github.com/tj-actions/cargo-bump). To create a release:

* git tag v0.1.0
* git push origin v0.1.0
* Create a GitHub Release — the workflow will bump `Cargo.toml` and build the deb package
