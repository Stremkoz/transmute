//! Executes validated conversion plans: scheduling, process supervision,
//! safe file finalisation, and progress events.

pub mod concurrency;
pub mod files;
pub mod run;
pub mod supervise;
