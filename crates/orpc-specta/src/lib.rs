mod schema;
mod export;

pub use schema::{specta, SpectaSchema, SpectaSchemaWrapper};
pub use export::{export_ts, generate_ts, ExportError};

// Re-export specta's Type derive macro for convenience.
pub use specta::Type;
