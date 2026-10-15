//! Worker-count policy.

use std::num::NonZeroUsize;
use std::thread;

/// Resolves how many conversions may run at once.
///
/// An explicit request wins; otherwise one worker per logical CPU. If the CPU
/// count cannot be determined (restricted containers, some sandboxes) we run
/// serially rather than guess high.
pub fn resolve_worker_count(requested: Option<NonZeroUsize>) -> NonZeroUsize {
    requested
        .or_else(|| thread::available_parallelism().ok())
        .unwrap_or(NonZeroUsize::MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_request_wins() {
        let three = NonZeroUsize::new(3).unwrap();
        assert_eq!(resolve_worker_count(Some(three)), three);
    }

    #[test]
    fn default_matches_available_parallelism() {
        let expected = thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
        assert_eq!(resolve_worker_count(None), expected);
    }
}
