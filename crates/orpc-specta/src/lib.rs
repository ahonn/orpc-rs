mod export;
mod schema;

pub use export::{ExportError, export_ts, generate_ts};
pub use schema::{SpectaSchema, SpectaSchemaWrapper, specta};

// Re-export specta's Type derive macro for convenience.
pub use specta::Type;
