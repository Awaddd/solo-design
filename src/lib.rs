//! Solo Design: sandboxed AI design agents.
//!
//! Modules are added phase by phase.

pub mod cli;
pub mod config;
pub mod engine;
pub mod engines;
pub mod inputs;
pub mod providers;
pub mod receipt;
pub mod root;
pub mod runtime;
pub mod signals;

/// One-line usage shown by the binary until the run command exists.
pub const USAGE: &str = "usage: solo-design run --agent <name> --task <folder> [--design <name>]";
