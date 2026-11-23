//! Input-policy core for the existing Windows Qt player.
//! No QEMU, Qt or Android code is replaced by this crate.

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NexusBinding {
    pub key: i32,
    pub x: f64,
    pub y: f64,
    pub repeat: u8,
    pub interval_ms: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NexusAimResult {
    pub x: f64,
    pub y: f64,
    pub reset: u32,
}

#[no_mangle]
pub extern "C" fn nexus_valid_key(key: i32) -> u8 {
    u8::from((b'A' as i32..=b'Z' as i32).contains(&key)
        || (b'0' as i32..=b'9' as i32).contains(&key)
        || key == b' ' as i32)
}

fn validate_bindings(bindings: &[NexusBinding]) -> u32 {
    if bindings.len() > 128 {
        return 1;
    }
    let mut modes: [Option<(u8, i32)>; 256] = [None; 256];
    for b in bindings {
        if nexus_valid_key(b.key) == 0
            || !b.x.is_finite()
            || !b.y.is_finite()
            || !(0.0..=1.0).contains(&b.x)
            || !(0.0..=1.0).contains(&b.y)
            || b.repeat > 1
            || !(100..=5000).contains(&b.interval_ms)
        {
            return 2;
        }
        // A valid key above is within this array's range.
        let slot = &mut modes[b.key as usize];
        let mode = (b.repeat, b.interval_ms);
        if slot.is_some_and(|existing| existing != mode) {
            return 3;
        }
        *slot = Some(mode);
    }
    0
}

/// Return 0 for success, 1 for too many, 2 for invalid fields,
/// 3 for inconsistent same-key repeat settings, 4 for null input.
///
/// # Safety
/// For nonzero `length`, `bindings` must point to `length` properly aligned,
/// initialized `NexusBinding` values valid for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn nexus_validate_bindings(bindings: *const NexusBinding, length: usize) -> u32 {
    if length > 128 {
        return 1;
    }
    if length == 0 {
        return 0;
    }
    if bindings.is_null() {
        return 4;
    }
    // SAFETY: the C++ caller supplies a live contiguous vector with `length` elements.
    validate_bindings(unsafe { std::slice::from_raw_parts(bindings, length) })
}

fn finite_clamp(value: f64, default: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() { value.clamp(min, max) } else { default }
}

#[no_mangle]
pub extern "C" fn nexus_aim_clamp_anchor(value: f64) -> f64 {
    finite_clamp(value, 0.5, 0.1, 0.9)
}

#[no_mangle]
pub extern "C" fn nexus_aim_clamp_sensitivity(value: f64) -> f64 {
    finite_clamp(value, 1.0, 0.1, 5.0)
}

/// Advance a single-pointer camera drag. At the edge, the caller must release
/// and press again at the anchor. This does not implement multitouch.
#[no_mangle]
pub extern "C" fn nexus_aim_move(
    anchor_x: f64, anchor_y: f64, x: f64, y: f64, gain: f64,
    dx: f64, dy: f64, width: i32, height: i32,
) -> NexusAimResult {
    let ax = nexus_aim_clamp_anchor(anchor_x);
    let ay = nexus_aim_clamp_anchor(anchor_y);
    let current_x = finite_clamp(x, ax, 0.0, 1.0);
    let current_y = finite_clamp(y, ay, 0.0, 1.0);
    if width <= 0 || height <= 0 || !dx.is_finite() || !dy.is_finite() {
        return NexusAimResult { x: current_x, y: current_y, reset: 0 };
    }
    let gain = nexus_aim_clamp_sensitivity(gain);
    let next_x = current_x + dx / f64::from(width) * gain;
    let next_y = current_y + dy / f64::from(height) * gain;
    if !(0.03..=0.97).contains(&next_x) || !(0.03..=0.97).contains(&next_y) {
        NexusAimResult { x: ax, y: ay, reset: 1 }
    } else {
        NexusAimResult { x: next_x, y: next_y, reset: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(key: i32, x: f64, y: f64, repeat: u8, interval_ms: i32) -> NexusBinding {
        NexusBinding { key, x, y, repeat, interval_ms }
    }

    #[test]
    fn accepts_old_keymap_rules_and_sequential_markers() {
        let b = [binding(65, 0.0, 1.0, 1, 100), binding(65, 0.9, 0.1, 1, 100), binding(32, 0.5, 0.5, 0, 250)];
        assert_eq!(validate_bindings(&b), 0);
        assert_eq!(unsafe { nexus_validate_bindings(b.as_ptr(), b.len()) }, 0);
        assert_eq!(unsafe { nexus_validate_bindings(std::ptr::null(), 0) }, 0);
        assert_eq!(unsafe { nexus_validate_bindings(std::ptr::null(), 1) }, 4);
        assert_eq!(unsafe { nexus_validate_bindings(std::ptr::null(), 129) }, 1);
    }

    #[test]
    fn rejects_bad_fields_and_conflicting_modes() {
        let valid = binding(65, 0.5, 0.5, 0, 250);
        for bad in [binding(13, 0.5, 0.5, 0, 250), binding(65, f64::NAN, 0.5, 0, 250),
                    binding(65, -0.1, 0.5, 0, 250), binding(65, 0.5, 0.5, 2, 250),
                    binding(65, 0.5, 0.5, 0, 99)] {
            assert_eq!(validate_bindings(&[bad]), 2);
        }
        assert_eq!(validate_bindings(&[valid, binding(65, 0.8, 0.5, 1, 250)]), 3);
    }

    #[test]
    fn aim_drag_edges_and_invalid_data() {
        let p = nexus_aim_move(0.5, 0.5, 0.5, 0.5, 1., 100., -50., 1000, 1000);
        assert!((p.x - 0.6).abs() < 0.00001 && (p.y - 0.45).abs() < 0.00001 && p.reset == 0);
        let p = nexus_aim_move(0.5, 0.5, 0.6, 0.45, 1., 1000., 0., 1000, 1000);
        assert_eq!((p.x, p.y, p.reset), (0.5, 0.5, 1));
        let p = nexus_aim_move(f64::NAN, 0.5, 0.5, 0.5, f64::INFINITY, f64::NAN, 2., 100, 100);
        assert_eq!((p.x, p.y, p.reset), (0.5, 0.5, 0));
        assert_eq!(nexus_aim_clamp_anchor(0.), 0.1);
        assert_eq!(nexus_aim_clamp_sensitivity(f64::NAN), 1.);
    }
}
