use std::sync::Arc;

use fontdb::Database;

pub const FONT_REGULAR_BYTES: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
pub const FONT_BOLD_BYTES: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.ttf");

pub fn init_font_db() -> Arc<Database> {
    let mut db = Database::new();
    db.load_font_data(FONT_REGULAR_BYTES.to_vec());
    db.load_font_data(FONT_BOLD_BYTES.to_vec());
    if db.len() < 2 {
        panic!(
            "failed to load Inter fonts into fontdb: expected 2 faces, got {}",
            db.len()
        );
    }
    Arc::new(db)
}
