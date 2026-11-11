//! C ABI boundary. Callers must initialize POD states with `*_init` before use.
//! Every non-null pointer must be aligned, valid for its complete state/Point,
//! and exclusively writable for the duration of a mutating call. Output Points
//! must not overlap a state. Null pointers are rejected without dereferencing.
//! This is the only module using unsafe; the solver implementation is safe Rust.
use crate::{AimSolver, AngularRadialSolver, Point};

#[repr(C)]
pub struct Matrix {
    pub values: [f64; 4],
}
#[repr(C)]
pub struct Layout {
    pub version: u32,
    pub aim_size: u32,
    pub aim_align: u32,
    pub angular_size: u32,
    pub angular_align: u32,
}
#[unsafe(no_mangle)]
pub extern "C" fn ewm_mlbb_core_layout() -> Layout {
    Layout {
        version: 1,
        aim_size: size_of::<AimSolver>() as u32,
        aim_align: align_of::<AimSolver>() as u32,
        angular_size: size_of::<AngularRadialSolver>() as u32,
        angular_align: align_of::<AngularRadialSolver>() as u32,
    }
}
unsafe fn write_point(out: *mut Point, value: Option<Point>) -> u8 {
    if out.is_null() {
        return 0;
    }
    match value {
        Some(point) => {
            // SAFETY: non-null `out` is a valid exclusive Point by the ABI contract.
            unsafe { out.write(point) };
            1
        }
        None => 0,
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_aim_init(state: *mut AimSolver) -> u8 {
    if state.is_null() {
        return 0;
    }
    // SAFETY: caller provides aligned, writable state storage; no previous value is read.
    unsafe { state.write(AimSolver::default()) };
    1
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_aim_reset(state: *mut AimSolver) {
    // SAFETY: any non-null state is initialized and exclusively borrowed by this call.
    if let Some(state) = unsafe { state.as_mut() } {
        state.reset();
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_aim_resume(state: *mut AimSolver, model: *const AimSolver) -> u8 {
    if state.is_null() || model.is_null() {
        return 0;
    }
    // SAFETY: read the initialized Copy model before borrowing state mutably.
    // This deliberately supports state == model, just like resumeFrom(*this).
    let mut snapshot = unsafe { model.read() };
    // Legacy resumeFrom(*this) resets its own deadzone before copying fields.
    // Preserve that observable edge case without ever aliasing Rust references.
    if std::ptr::eq(state.cast_const(), model) {
        snapshot.deadzone = 0.;
    }
    unsafe { &mut *state }.resume_from(&snapshot) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_aim_predict(
    state: *const AimSolver,
    desired: Point,
    center: Point,
    radius: f64,
    out: *mut Point,
) -> u8 {
    // SAFETY: non-null state is an immutable initialized state for this call.
    let Some(state) = (unsafe { state.as_ref() }) else {
        return 0;
    };
    unsafe { write_point(out, state.predict(desired, center, radius)) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_aim_consistent(
    state: *const AimSolver,
    touch: Point,
    observed: Point,
    center: Point,
    noise: f64,
) -> u8 {
    // SAFETY: non-null state is an immutable initialized state for this call.
    let Some(state) = (unsafe { state.as_ref() }) else {
        return 0;
    };
    state.observation_consistent(touch, observed, center, noise) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_aim_step(
    state: *mut AimSolver,
    touch: Point,
    observed: Point,
    desired: Point,
    center: Point,
    radius: f64,
    coherent: u8,
    adapt: u8,
    out: *mut Point,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: non-null state is initialized/exclusive; output does not overlap it.
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    unsafe {
        write_point(
            out,
            state.step(
                touch,
                observed,
                desired,
                center,
                radius,
                coherent != 0,
                adapt != 0,
            ),
        )
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_angular_init(state: *mut AngularRadialSolver) -> u8 {
    if state.is_null() {
        return 0;
    }
    // SAFETY: caller provides aligned writable storage; no previous value is read.
    unsafe { state.write(AngularRadialSolver::default()) };
    1
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_angular_reset(state: *mut AngularRadialSolver) {
    // SAFETY: non-null state is initialized and exclusively borrowed by this call.
    if let Some(state) = unsafe { state.as_mut() } {
        state.reset();
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_angular_configure(
    state: *mut AngularRadialSolver,
    center: Point,
    radius: f64,
    camera: Matrix,
    range: f64,
    resolution: Point,
) -> u8 {
    // SAFETY: non-null state is initialized and exclusively borrowed by this call.
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    state.configure(center, radius, camera.values, range, resolution) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_angular_predict(
    state: *const AngularRadialSolver,
    desired: Point,
    out: *mut Point,
) -> u8 {
    // SAFETY: non-null state is initialized/immutable; output does not overlap it.
    let Some(state) = (unsafe { state.as_ref() }) else {
        return 0;
    };
    unsafe { write_point(out, state.predict(desired)) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_angular_forward(
    state: *const AngularRadialSolver,
    command: Point,
    out: *mut Point,
) -> u8 {
    // SAFETY: non-null state is initialized/immutable; output does not overlap it.
    let Some(state) = (unsafe { state.as_ref() }) else {
        return 0;
    };
    unsafe { write_point(out, state.forward(command)) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_angular_clamped(
    state: *const AngularRadialSolver,
    desired: Point,
    out: *mut Point,
) -> u8 {
    // SAFETY: non-null state is initialized/immutable; output does not overlap it.
    let Some(state) = (unsafe { state.as_ref() }) else {
        return 0;
    };
    unsafe { write_point(out, Some(state.clamped_world(desired))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_null_boundary_pointers() {
        // SAFETY: each deliberately null pointer must be rejected before access.
        unsafe {
            assert_eq!(ewm_mlbb_aim_init(std::ptr::null_mut()), 0);
            assert_eq!(
                ewm_mlbb_aim_resume(std::ptr::null_mut(), std::ptr::null()),
                0
            );
            assert_eq!(
                ewm_mlbb_angular_configure(
                    std::ptr::null_mut(),
                    Point::default(),
                    10.,
                    Matrix {
                        values: [1., 0., 0., 1.]
                    },
                    1.,
                    Point::new(100., 100.)
                ),
                0
            );
            let mut state = AimSolver::default();
            let before = state.stage;
            assert_eq!(
                ewm_mlbb_aim_step(
                    &mut state,
                    Point::default(),
                    Point::default(),
                    Point::default(),
                    Point::default(),
                    10.,
                    1,
                    1,
                    std::ptr::null_mut()
                ),
                0
            );
            assert_eq!(state.stage, before);
        }
    }
}
