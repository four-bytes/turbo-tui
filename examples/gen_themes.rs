//! Generate JSON theme files in the `themes/` directory.
//!
//! Run with: `cargo run --example gen_themes --features json-themes`
//!
//! This only generates the built-in Turbo Vision theme.
//! Other themes (Dark, Modern, Matrix, Windows) are hand-maintained JSON files.

use std::path::Path;
use turbo_tui::theme::Theme;

fn main() {
    let themes_dir = Path::new("themes");
    std::fs::create_dir_all(themes_dir).expect("Failed to create themes directory");

    // Generate only the built-in Turbo Vision theme
    let path = themes_dir.join("turbo-vision.json");
    Theme::turbo_vision()
        .save_json(&path, "Turbo Vision")
        .unwrap_or_else(|e| panic!("Failed to save turbo-vision.json: {e}"));
    println!("Generated: {}", path.display());
}
