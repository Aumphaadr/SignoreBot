//! Конфигурация: схема, миграция, хранилище.

pub mod migrate;
pub mod schema;
pub mod store;

pub use schema::*;

/// Конфиг, разделяемый между подсистемами.
pub type SharedConfig = std::sync::Arc<parking_lot::RwLock<Config>>;
