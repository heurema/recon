pub mod config;
pub mod error;
pub mod model;

pub use model::{Briefing, Section, SourceError, SourceResult};
pub use config::{Config, Source};
