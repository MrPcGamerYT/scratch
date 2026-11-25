//! Input-policy and touch-geometry core for the Windows Qt player.
//! Owns VM launch policy and WHPX preflight as well as input geometry.
//! QEMU remains the VM backend; Qt remains the UI. No external crates are used.

pub mod runtime;
pub mod input;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NexusBinding {
    pub key: i32,
    pub x: f64,
    pub y: f64,
    pub repeat: u8,
    pub burst_count: u8,
    pub reserved: u16,
    pub interval_ms: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NexusAimResult {
    pub x: f64,
    pub y: f64,
    pub reset: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NexusPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NexusPinchPair {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Script operation codes:
/// 0 = Tap, 1 = Wait, 2 = Swipe, 3 = Down, 4 = Move, 5 = Up, 6 = MultiPress
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NexusScriptStep {
    pub op: u8,
    pub reserved: [u8; 3],
    pub duration_ms: i32,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Supported Win32 virtual keys and synthetic mouse binding codes (0x101..=0x107).
/// System keys such as Escape (0x1B) and Left/Right Windows (0x5B/0x5C) are excluded.
#[no_mangle]
pub extern "C" fn nexus_valid_key(key: i32) -> u8 {
    let valid = matches!(
        key,
        0x08 | 0x09 | 0x0D                       // Backspace, Tab, Enter
        | 0x10..=0x12 | 0x14                     // Shift, Ctrl, Alt, CapsLock
        | 0x20..=0x28                            // Space, PgUp, PgDn, End, Home, Arrows
        | 0x2D | 0x2E                            // Insert, Delete
        | 0x30..=0x39                            // 0..9
        | 0x41..=0x5A                            // A..Z
        | 0x60..=0x6B | 0x6D..=0x6F              // Numpad 0..9 and operators
        | 0x70..=0x7B                            // F1..F12
        | 0xA0..=0xA5                            // L/R Shift, Ctrl, Alt
        | 0xBA..=0xC0 | 0xDB..=0xDE              // Punctuation / OEM keys
        | 0x101..=0x107                          // Mouse Right, Middle, X1, X2, WheelUp, WheelDown
    );
    // F11 belongs to the player fullscreen action, including embedded guest focus.
    u8::from(valid && key != 0x7A)
}

fn valid_unit(v: f64) -> bool {
    v.is_finite() && (0.0..=1.0).contains(&v)
}

fn validate_bindings(bindings: &[NexusBinding]) -> u32 {
    if bindings.len() > 128 {
        return 1;
    }
    let mut modes: [Option<(u8, u8, i32)>; 512] = [None; 512];
    for b in bindings {
        let burst = if b.burst_count == 0 { 1 } else { b.burst_count };
        if nexus_valid_key(b.key) == 0
            || !valid_unit(b.x)
            || !valid_unit(b.y)
            || b.repeat > 1
            || burst > 50
            || (b.repeat == 1 && burst > 1)
            || !(25..=5000).contains(&b.interval_ms)
        {
            return 2;
        }
        let slot = &mut modes[b.key as usize];
        let mode = (b.repeat, burst, b.interval_ms);
        if slot.is_some_and(|existing| existing != mode) {
            return 3;
        }
        *slot = Some(mode);
    }
    0
}

/// Return 0 for success, 1 for too many, 2 for invalid fields,
/// 3 for inconsistent same-key repeat/burst settings, 4 for null input.
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
    finite_clamp(value, 1.0, 0.05, 10.0)
}

/// Advance a camera drag with independent X and Y sensitivity. At the screen
/// margin, the caller must release and re-press at the anchor.
#[no_mangle]
pub extern "C" fn nexus_aim_move_xy(
    anchor_x: f64, anchor_y: f64, x: f64, y: f64, gain_x: f64, gain_y: f64,
    dx: f64, dy: f64, width: i32, height: i32,
) -> NexusAimResult {
    let ax = nexus_aim_clamp_anchor(anchor_x);
    let ay = nexus_aim_clamp_anchor(anchor_y);
    let current_x = finite_clamp(x, ax, 0.0, 1.0);
    let current_y = finite_clamp(y, ay, 0.0, 1.0);
    if width <= 0 || height <= 0 || !dx.is_finite() || !dy.is_finite() {
        return NexusAimResult { x: current_x, y: current_y, reset: 0 };
    }
    let gx = nexus_aim_clamp_sensitivity(gain_x);
    let gy = nexus_aim_clamp_sensitivity(gain_y);
    let next_x = current_x + dx / f64::from(width) * gx;
    let next_y = current_y + dy / f64::from(height) * gy;
    if !(0.03..=0.97).contains(&next_x) || !(0.03..=0.97).contains(&next_y) {
        NexusAimResult { x: ax, y: ay, reset: 1 }
    } else {
        NexusAimResult { x: next_x, y: next_y, reset: 0 }
    }
}

/// Uniform-sensitivity wrapper preserved for existing callers.
#[no_mangle]
pub extern "C" fn nexus_aim_move(
    anchor_x: f64, anchor_y: f64, x: f64, y: f64, gain: f64,
    dx: f64, dy: f64, width: i32, height: i32,
) -> NexusAimResult {
    nexus_aim_move_xy(anchor_x, anchor_y, x, y, gain, gain, dx, dy, width, height)
}

/// Compute normalized 8-way virtual joystick target position around `(center_x, center_y)`.
/// Diagonal motion is scaled by `1 / sqrt(2)` so diagonal speed never exceeds cardinal radius.
#[no_mangle]
pub extern "C" fn nexus_dpad_target(
    center_x: f64, center_y: f64, radius: f64,
    up: u8, down: u8, left: u8, right: u8,
    modifier_scale: f64,
) -> NexusPoint {
    let cx = finite_clamp(center_x, 0.2, 0.02, 0.98);
    let cy = finite_clamp(center_y, 0.75, 0.02, 0.98);
    let r = finite_clamp(radius, 0.10, 0.02, 0.45);
    let scale = finite_clamp(modifier_scale, 1.0, 0.25, 2.5);
    let ix = i32::from(right != 0) - i32::from(left != 0);
    let iy = i32::from(down != 0) - i32::from(up != 0);
    if ix == 0 && iy == 0 {
        return NexusPoint { x: cx, y: cy };
    }
    let norm = if ix != 0 && iy != 0 { std::f64::consts::FRAC_1_SQRT_2 } else { 1.0 };
    let reach = r * scale * norm;
    NexusPoint {
        x: (cx + f64::from(ix) * reach).clamp(0.01, 0.99),
        y: (cy + f64::from(iy) * reach).clamp(0.01, 0.99),
    }
}

/// Smoothstep-interpolated point along a swipe gesture for `progress` in `[0.0, 1.0]`.
#[no_mangle]
pub extern "C" fn nexus_swipe_point(
    x1: f64, y1: f64, x2: f64, y2: f64, progress: f64,
) -> NexusPoint {
    let sx = finite_clamp(x1, 0.5, 0.0, 1.0);
    let sy = finite_clamp(y1, 0.5, 0.0, 1.0);
    let ex = finite_clamp(x2, 0.5, 0.0, 1.0);
    let ey = finite_clamp(y2, 0.5, 0.0, 1.0);
    let t = finite_clamp(progress, 0.0, 0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    NexusPoint {
        x: (sx + (ex - sx) * s).clamp(0.0, 1.0),
        y: (sy + (ey - sy) * s).clamp(0.0, 1.0),
    }
}

/// Symmetric two-finger diagonal coordinates for pinch-out (`zoom_in != 0`) or pinch-in (`zoom_in == 0`).
#[no_mangle]
pub extern "C" fn nexus_pinch_points(
    center_x: f64, center_y: f64, radius: f64, progress: f64, zoom_in: u8,
) -> NexusPinchPair {
    let cx = finite_clamp(center_x, 0.5, 0.05, 0.95);
    let cy = finite_clamp(center_y, 0.5, 0.05, 0.95);
    let outer = finite_clamp(radius, 0.14, 0.04, 0.45);
    let inner = (outer * 0.22).max(0.015);
    let t = finite_clamp(progress, 0.0, 0.0, 1.0);
    let span = if zoom_in != 0 {
        inner + (outer - inner) * t
    } else {
        outer + (inner - outer) * t
    };
    let diag = span * std::f64::consts::FRAC_1_SQRT_2;
    NexusPinchPair {
        x1: (cx - diag).clamp(0.01, 0.99),
        y1: (cy + diag).clamp(0.01, 0.99),
        x2: (cx + diag).clamp(0.01, 0.99),
        y2: (cy - diag).clamp(0.01, 0.99),
    }
}

/// Project host cursor direction and proportional range relative to `(origin_x, origin_y)`
/// onto a MOBA skill wheel centered at `(skill_x, skill_y)` with `radius`.
#[no_mangle]
pub extern "C" fn nexus_moba_target(
    skill_x: f64, skill_y: f64, radius: f64,
    origin_x: f64, origin_y: f64,
    cursor_x: f64, cursor_y: f64,
) -> NexusPoint {
    let sx = finite_clamp(skill_x, 0.8, 0.02, 0.98);
    let sy = finite_clamp(skill_y, 0.75, 0.02, 0.98);
    let r = finite_clamp(radius, 0.11, 0.02, 0.35);
    let ox = finite_clamp(origin_x, 0.5, 0.0, 1.0);
    let oy = finite_clamp(origin_y, 0.5, 0.0, 1.0);
    let mx = finite_clamp(cursor_x, ox, 0.0, 1.0);
    let my = finite_clamp(cursor_y, oy, 0.0, 1.0);
    let dx = mx - ox;
    let dy = my - oy;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 1e-6 {
        return NexusPoint { x: sx, y: sy };
    }
    let ratio = (dist / 0.35).min(1.0);
    let scale = (r * ratio) / dist;
    NexusPoint {
        x: (sx + dx * scale).clamp(0.01, 0.99),
        y: (sy + dy * scale).clamp(0.01, 0.99),
    }
}

fn validate_script_steps(steps: &[NexusScriptStep]) -> u32 {
    if steps.is_empty() || steps.len() > 256 {
        return 1;
    }
    for s in steps {
        match s.op {
            0 => {
                // Tap: x1, y1 in [0..1], hold duration 10..10000 ms
                if !valid_unit(s.x1) || !valid_unit(s.y1) || !(10..=10_000).contains(&s.duration_ms) {
                    return 2;
                }
            }
            1 => {
                // Wait: duration 10..60000 ms
                if !(10..=60_000).contains(&s.duration_ms) {
                    return 2;
                }
            }
            2 | 6 => {
                // Swipe (2) or MultiPress (6): two coordinate pairs + duration
                if !valid_unit(s.x1)
                    || !valid_unit(s.y1)
                    || !valid_unit(s.x2)
                    || !valid_unit(s.y2)
                    || !(20..=10_000).contains(&s.duration_ms)
                {
                    return 2;
                }
            }
            3 | 4 => {
                // Down (3) or Move (4): x1, y1 in [0..1]
                if !valid_unit(s.x1) || !valid_unit(s.y1) {
                    return 2;
                }
            }
            5 => {
                // Up (5): no extra parameters needed
            }
            _ => return 2,
        }
    }
    0
}

/// Validate a compiled script step buffer.
/// Return 0 for valid, 1 for empty or >256 steps, 2 for invalid op/coordinates/duration, 4 for null.
///
/// # Safety
/// For nonzero `length`, `steps` must point to `length` valid `NexusScriptStep` elements.
#[no_mangle]
pub unsafe extern "C" fn nexus_validate_script_steps(steps: *const NexusScriptStep, length: usize) -> u32 {
    if length == 0 || length > 256 {
        return 1;
    }
    if steps.is_null() {
        return 4;
    }
    validate_script_steps(unsafe { std::slice::from_raw_parts(steps, length) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    fn binding(key: i32, x: f64, y: f64, repeat: u8, interval_ms: i32) -> NexusBinding {
        NexusBinding { key, x, y, repeat, burst_count: 1, reserved: 0, interval_ms }
    }

    #[test]
    fn c_abi_sizes_are_stable() {
        assert_eq!((size_of::<NexusBinding>(), align_of::<NexusBinding>()), (32, 8));
        assert_eq!((size_of::<NexusAimResult>(), align_of::<NexusAimResult>()), (24, 8));
        assert_eq!((size_of::<NexusPoint>(), align_of::<NexusPoint>()), (16, 8));
        assert_eq!((size_of::<NexusPinchPair>(), align_of::<NexusPinchPair>()), (32, 8));
        assert_eq!((size_of::<NexusScriptStep>(), align_of::<NexusScriptStep>()), (40, 8));
    }

    #[test]
    fn accepts_extended_keys_bursts_and_sequential_markers() {
        let mut burst = binding(0x101, 0.4, 0.6, 0, 60); // MouseRight burst
        burst.burst_count = 5;
        let b = [
            binding(65, 0.0, 1.0, 1, 100),
            binding(65, 0.9, 0.1, 1, 100),
            binding(32, 0.5, 0.5, 0, 250),
            binding(0x70, 0.2, 0.3, 1, 25), // F1 at 25 ms turbo
            burst,
        ];
        assert_eq!(validate_bindings(&b), 0);
        assert_eq!(unsafe { nexus_validate_bindings(b.as_ptr(), b.len()) }, 0);
        assert_eq!(unsafe { nexus_validate_bindings(std::ptr::null(), 0) }, 0);
        assert_eq!(unsafe { nexus_validate_bindings(std::ptr::null(), 1) }, 4);
        assert_eq!(unsafe { nexus_validate_bindings(std::ptr::null(), 129) }, 1);
    }

    #[test]
    fn rejects_reserved_keys_bad_fields_and_conflicting_modes() {
        assert_eq!(nexus_valid_key(0x7A), 0); // F11 reserved for fullscreen
        assert_eq!(nexus_valid_key(0x1B), 0); // Escape disallowed
        assert_eq!(nexus_valid_key(0x5B), 0); // Left Win disallowed
        assert_eq!(nexus_valid_key(0x0D), 1); // Enter allowed
        assert_eq!(nexus_valid_key(0x105), 1); // WheelUp allowed

        let valid = binding(65, 0.5, 0.5, 0, 250);
        let mut both_repeat_and_burst = binding(65, 0.5, 0.5, 1, 100);
        both_repeat_and_burst.burst_count = 3;
        for bad in [
            binding(0x1B, 0.5, 0.5, 0, 250),
            binding(65, f64::NAN, 0.5, 0, 250),
            binding(65, -0.1, 0.5, 0, 250),
            binding(65, 0.5, 0.5, 2, 250),
            binding(65, 0.5, 0.5, 0, 24),
            both_repeat_and_burst,
        ] {
            assert_eq!(validate_bindings(&[bad]), 2);
        }
        assert_eq!(validate_bindings(&[valid, binding(65, 0.8, 0.5, 1, 250)]), 3);
    }

    #[test]
    fn aim_xy_dpad_swipe_pinch_moba_and_script_math() {
        let p = nexus_aim_move_xy(0.5, 0.5, 0.5, 0.5, 1.0, 2.0, 100.0, -50.0, 1000, 1000);
        assert!((p.x - 0.6).abs() < 1e-6 && (p.y - 0.4).abs() < 1e-6 && p.reset == 0);
        let edge = nexus_aim_move(0.5, 0.5, 0.6, 0.45, 1.0, 1000.0, 0.0, 1000, 1000);
        assert_eq!((edge.x, edge.y, edge.reset), (0.5, 0.5, 1));
        assert_eq!(nexus_aim_clamp_anchor(0.0), 0.1);
        assert_eq!(nexus_aim_clamp_sensitivity(f64::NAN), 1.0);

        // 8-way diagonal normalization and sprint scaling
        let diag = nexus_dpad_target(0.5, 0.5, 0.10, 1, 0, 0, 1, 1.0);
        let dist = ((diag.x - 0.5).powi(2) + (diag.y - 0.5).powi(2)).sqrt();
        assert!((dist - 0.10).abs() < 1e-6 && diag.x > 0.5 && diag.y < 0.5);
        let sprint = nexus_dpad_target(0.5, 0.5, 0.10, 0, 0, 0, 1, 1.5);
        assert!((sprint.x - 0.65).abs() < 1e-6);

        // Swipe midpoint and endpoints
        let sw0 = nexus_swipe_point(0.2, 0.8, 0.6, 0.4, 0.0);
        let sw5 = nexus_swipe_point(0.2, 0.8, 0.6, 0.4, 0.5);
        let sw1 = nexus_swipe_point(0.2, 0.8, 0.6, 0.4, 1.0);
        assert!((sw0.x - 0.2).abs() < 1e-9 && (sw0.y - 0.8).abs() < 1e-9);
        assert!((sw5.x - 0.4).abs() < 1e-9 && (sw5.y - 0.6).abs() < 1e-9);
        assert!((sw1.x - 0.6).abs() < 1e-9 && (sw1.y - 0.4).abs() < 1e-9);

        // Pinch spread increases finger separation
        let start = nexus_pinch_points(0.5, 0.5, 0.20, 0.0, 1);
        let end = nexus_pinch_points(0.5, 0.5, 0.20, 1.0, 1);
        assert!((end.x2 - end.x1) > (start.x2 - start.x1));

        // MOBA target aims right when cursor is right of hero origin
        let cast = nexus_moba_target(0.80, 0.75, 0.10, 0.50, 0.50, 0.90, 0.50);
        assert!((cast.x - 0.90).abs() < 1e-6 && (cast.y - 0.75).abs() < 1e-6);

        // Script validation
        let steps = [
            NexusScriptStep { op: 0, reserved: [0; 3], duration_ms: 50, x1: 0.5, y1: 0.5, x2: 0.0, y2: 0.0 },
            NexusScriptStep { op: 1, reserved: [0; 3], duration_ms: 100, x1: 0.0, y1: 0.0, x2: 0.0, y2: 0.0 },
            NexusScriptStep { op: 2, reserved: [0; 3], duration_ms: 180, x1: 0.2, y1: 0.8, x2: 0.2, y2: 0.3 },
        ];
        assert_eq!(unsafe { nexus_validate_script_steps(steps.as_ptr(), steps.len()) }, 0);
        let bad_step = [NexusScriptStep { op: 9, reserved: [0; 3], duration_ms: 50, x1: 0.5, y1: 0.5, x2: 0.0, y2: 0.0 }];
        assert_eq!(unsafe { nexus_validate_script_steps(bad_step.as_ptr(), 1) }, 2);
    }
}
