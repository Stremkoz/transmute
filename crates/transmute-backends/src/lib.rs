//! External tool discovery, capability probing, and per-tool adapters.
//!
//! Encoders run as child processes behind adapters so cancellation is a real
//! kill and a crashing codec cannot take the application down.

pub mod catalog;
pub mod discovery;
pub mod probe;
pub mod process;
pub mod version;
