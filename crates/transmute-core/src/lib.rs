//! Domain model and planning logic for Transmute.
//!
//! This crate is deliberately free of process spawning and async runtime
//! dependencies so that inventory, planning, and path/collision policy can be
//! unit-tested without any external tools installed.

pub mod capability;
pub mod codec;
pub mod extension;
pub mod format;
pub mod inspect;
pub mod inventory;
pub mod naming;
pub mod output;
pub mod plan;
pub mod preset;
pub mod schema;
pub mod tool;
pub mod validation;
