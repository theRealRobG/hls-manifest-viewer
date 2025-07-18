use hls_manifest_viewer::App;
use leptos::prelude::*;

fn main() {
    init_log();
    console_error_panic_hook::set_once();
    mount_to_body(App)
}

#[cfg(feature = "console_log")]
fn init_log() {
    _ = console_log::init_with_level(log::Level::Debug);
}
#[cfg(not(feature = "console_log"))]
fn init_log() {}
