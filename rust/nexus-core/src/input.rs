//! Bounded host input policy. These are batching controls, not FPS guarantees.
#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct InputDelta { pub x: i32, pub y: i32 }

#[no_mangle]
pub extern "C" fn nexus_input_accumulate(x: i32, y: i32, dx: i32, dy: i32) -> InputDelta {
    InputDelta { x: x.saturating_add(dx).clamp(-32768, 32768), y: y.saturating_add(dy).clamp(-32768, 32768) }
}

#[no_mangle]
pub extern "C" fn nexus_input_interval(requested: u32) -> u32 {
    match requested { 4 | 8 | 16 => requested, _ => 16 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn preserves_normal_counts() {
        assert_eq!(nexus_input_accumulate(12, -8, 7, -2), InputDelta { x: 19, y: -10 });
        assert_eq!(nexus_input_accumulate(19, -10, -19, 10), InputDelta { x: 0, y: 0 });
    }
    #[test] fn bounds_extreme_and_overflowing_counts() {
        assert_eq!(nexus_input_accumulate(i32::MAX, i32::MIN, i32::MAX, i32::MIN), InputDelta { x: 32768, y: -32768 });
    }
    #[test] fn only_supported_batch_intervals() {
        for value in [4, 8, 16] { assert_eq!(nexus_input_interval(value), value); }
        for value in [0, 1, 3, 100, u32::MAX] { assert_eq!(nexus_input_interval(value), 16); }
    }
}
