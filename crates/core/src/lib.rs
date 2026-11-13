//! Pure domain core for the movie renamer.
//!
//! This crate performs no filesystem, network, database, or clock I/O. Every
//! function is deterministic and unit-testable.

pub mod formatter;
pub mod matcher;
pub mod model;
pub mod parser;
pub mod planner;
pub mod sanitizer;

pub use formatter::{FormatOptions, FormattedName, NameFormatter};
pub use matcher::{best_match, score_candidate, MatchThresholds};
pub use model::*;
pub use parser::{parse_filename, parse_filename_with_context, parse_path};
pub use planner::{PlanInput, Planner, SidecarInput, VideoPlanInput};
pub use sanitizer::{sanitize_component, truncate_component};
