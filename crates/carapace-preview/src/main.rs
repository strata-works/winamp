//! carapace-preview — a live, interactive browser previewer for carapace skins.
//! See docs/superpowers/specs/2026-07-01-carapace-preview-design.md.

mod preview_host;
mod protocol;
mod render;
mod skin_session;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(skin_dir) = args.next() else {
        eprintln!("usage: carapace-preview <skin-dir> [--port <n>]");
        return ExitCode::FAILURE;
    };
    println!("carapace-preview: {skin_dir}");
    ExitCode::SUCCESS
}
