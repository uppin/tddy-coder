//! PRD acceptance: Telegram **recipe** keyboard lists include **`review`** (normalized CLI name) where extended recipe pages are defined.
//!
//! Fails until `telegram_session_control.rs` (or the canonical recipe keyboard module) lists `review`.

use std::fs;
use std::path::Path;

#[test]
fn telegram_recipe_more_includes_review() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let canonical = manifest_dir.join("src/telegram_session_control.rs");
    assert!(
        canonical.is_file(),
        "expected daemon recipe keyboard source at {} (define RECIPE_MORE_PAGE / extended recipe list here)",
        canonical.display()
    );
    let src = fs::read_to_string(&canonical).expect("read telegram_session_control.rs");
    assert!(
        src.contains("review"),
        "{} must include the `review` recipe for Telegram selection parity",
        canonical.display()
    );
}
