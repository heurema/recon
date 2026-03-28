/// Delta computation between two consecutive source data snapshots.
///
/// Counts are based on comparing JSON values:
/// - If both are arrays of objects: element-level diff (added/removed/changed)
/// - Otherwise: treat as a single atomic value (changed=1 if different)
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Delta {
    /// Number of new items added since last run.
    pub added: usize,
    /// Number of items removed since last run.
    pub removed: usize,
    /// Number of items that changed since last run.
    pub changed: usize,
    /// True on the very first run for this source (no previous history).
    pub new: bool,
    /// True when 5+ consecutive runs produced identical data.
    pub stalled: bool,
}

impl Delta {
    /// Construct a "first run" delta.
    pub fn first_run() -> Self {
        Delta {
            added: 0,
            removed: 0,
            changed: 0,
            new: true,
            stalled: false,
        }
    }

    /// Compute delta between previous and current data.
    /// `identical_run_count` is the count of consecutive identical runs
    /// (including the current one) used to determine `stalled`.
    pub fn compute(
        prev: &Value,
        current: &Value,
        identical_run_count: usize,
    ) -> Self {
        let (added, removed, changed) = value_diff(prev, current);
        let stalled = identical_run_count >= 5;
        Delta {
            added,
            removed,
            changed,
            new: false,
            stalled,
        }
    }
}

/// Compare two JSON values and return (added, removed, changed) counts.
fn value_diff(prev: &Value, current: &Value) -> (usize, usize, usize) {
    match (prev, current) {
        (Value::Array(p), Value::Array(c)) => array_diff(p, c),
        _ => {
            // Atomic comparison
            if prev == current {
                (0, 0, 0)
            } else {
                (0, 0, 1)
            }
        }
    }
}

/// Diff two JSON arrays using a stable key strategy:
/// - Elements are matched by position.
/// - Extra elements in `current` → added.
/// - Extra elements in `prev` → removed.
/// - Same-position elements that differ → changed.
fn array_diff(prev: &[Value], current: &[Value]) -> (usize, usize, usize) {
    let min_len = prev.len().min(current.len());
    let mut changed = 0;
    for i in 0..min_len {
        if prev[i] != current[i] {
            changed += 1;
        }
    }
    let added = current.len().saturating_sub(prev.len());
    let removed = prev.len().saturating_sub(current.len());
    (added, removed, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_first_run_delta() {
        let d = Delta::first_run();
        assert!(d.new);
        assert!(!d.stalled);
        assert_eq!(d.added, 0);
        assert_eq!(d.removed, 0);
        assert_eq!(d.changed, 0);
    }

    #[test]
    fn test_identical_values_no_diff() {
        let prev = json!({"x": 1});
        let cur = json!({"x": 1});
        let d = Delta::compute(&prev, &cur, 1);
        assert!(!d.new);
        assert!(!d.stalled);
        assert_eq!(d.added, 0);
        assert_eq!(d.removed, 0);
        assert_eq!(d.changed, 0);
    }

    #[test]
    fn test_atomic_change() {
        let prev = json!("old");
        let cur = json!("new");
        let d = Delta::compute(&prev, &cur, 1);
        assert_eq!(d.changed, 1);
        assert_eq!(d.added, 0);
        assert_eq!(d.removed, 0);
    }

    #[test]
    fn test_array_added() {
        let prev = json!([1, 2]);
        let cur = json!([1, 2, 3]);
        let d = Delta::compute(&prev, &cur, 1);
        assert_eq!(d.added, 1);
        assert_eq!(d.removed, 0);
        assert_eq!(d.changed, 0);
    }

    #[test]
    fn test_array_removed() {
        let prev = json!([1, 2, 3]);
        let cur = json!([1, 2]);
        let d = Delta::compute(&prev, &cur, 1);
        assert_eq!(d.removed, 1);
        assert_eq!(d.added, 0);
        assert_eq!(d.changed, 0);
    }

    #[test]
    fn test_array_changed() {
        let prev = json!([1, 2, 3]);
        let cur = json!([1, 99, 3]);
        let d = Delta::compute(&prev, &cur, 1);
        assert_eq!(d.changed, 1);
        assert_eq!(d.added, 0);
        assert_eq!(d.removed, 0);
    }

    #[test]
    fn test_stalled_at_5() {
        let prev = json!({"x": 1});
        let cur = json!({"x": 1});
        let d = Delta::compute(&prev, &cur, 5);
        assert!(d.stalled);
    }

    #[test]
    fn test_not_stalled_below_5() {
        let prev = json!({"x": 1});
        let cur = json!({"x": 1});
        let d = Delta::compute(&prev, &cur, 4);
        assert!(!d.stalled);
    }

    #[test]
    fn test_delta_serialization() {
        let d = Delta {
            added: 2,
            removed: 1,
            changed: 0,
            new: false,
            stalled: true,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"added\":2"));
        assert!(json.contains("\"stalled\":true"));
    }
}
