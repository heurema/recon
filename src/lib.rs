pub mod config;
pub mod error;
pub mod exec;
pub mod model;
pub mod runner;

pub use model::{Briefing, Section, SourceError, SourceResult};
pub use config::{Config, Source};
