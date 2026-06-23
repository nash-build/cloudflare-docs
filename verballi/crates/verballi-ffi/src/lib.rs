//! C-compatible FFI for `verballi`.
//!
//! This is the stable boundary native shells link against:
//! * **macOS / iOS** — link the `staticlib`, call from Swift via a bridging
//!   header (see `include/verballi.h`).
//! * **Android** — link the `cdylib` (`libverballi_ffi.so`) and call via JNI,
//!   or generate Kotlin bindings.
//! * **Windows / Linux desktop** — link the `cdylib`/`staticlib` from C/C++.
//!
//! ## Lifecycle
//! ```c
//! VcEngine* e = vc_engine_new(48000);
//! vc_engine_begin_enrollment(e);
//! vc_engine_process(e, mic, mic_len, out, out_cap, &written); // during enroll
//! int32_t dim = vc_engine_finish_enrollment(e, profile, profile_cap, &plen);
//! // ... later ...
//! vc_engine_process(e, mic, mic_len, out, out_cap, &written);  // isolated audio
//! vc_engine_free(e);
//! ```
//!
//! All buffers are caller-owned `float`/`uint8` arrays; the library never
//! retains pointers past a call. Functions return [`VC_OK`] or a negative error.

use std::os::raw::c_int;
use std::ptr;
use std::slice;

use verballi::{Config, SpeakerProfile, VoiceEngine};

/// Opaque engine handle.
pub struct VcEngine {
    inner: VoiceEngine,
    /// The most recently finished enrollment, serialised, so the
    /// size-query-then-fetch calling pattern works without re-enrolling.
    last_profile: Option<Vec<u8>>,
}

/// Success.
pub const VC_OK: c_int = 0;
/// A required pointer argument was null.
pub const VC_ERR_NULL: c_int = -1;
/// The configuration was rejected.
pub const VC_ERR_CONFIG: c_int = -2;
/// The provided output buffer was too small.
pub const VC_ERR_BUFFER: c_int = -3;
/// The engine was not in the expected state (e.g. not enrolling).
pub const VC_ERR_STATE: c_int = -4;

/// Create an engine for the given input sample rate (Hz). Returns null on
/// invalid configuration. Free with [`vc_engine_free`].
///
/// # Safety
/// The returned pointer must be freed exactly once with [`vc_engine_free`].
#[no_mangle]
pub extern "C" fn vc_engine_new(input_sample_rate: u32) -> *mut VcEngine {
    let mut cfg = Config::default();
    cfg.input_sample_rate = input_sample_rate;
    match VoiceEngine::new(cfg) {
        Ok(inner) => Box::into_raw(Box::new(VcEngine { inner, last_profile: None })),
        Err(_) => ptr::null_mut(),
    }
}

/// Create an engine from explicit tuning parameters. Returns null if rejected.
///
/// # Safety
/// See [`vc_engine_new`].
#[no_mangle]
pub extern "C" fn vc_engine_new_tuned(
    input_sample_rate: u32,
    highpass_hz: f32,
    denoise_strength: f32,
    speaker_focus: f32,
    proximity_focus: f32,
) -> *mut VcEngine {
    let mut cfg = Config::default();
    cfg.input_sample_rate = input_sample_rate;
    cfg.highpass_hz = highpass_hz;
    cfg.denoise_strength = denoise_strength;
    cfg.speaker_focus = speaker_focus;
    cfg.proximity_focus = proximity_focus;
    match VoiceEngine::new(cfg) {
        Ok(inner) => Box::into_raw(Box::new(VcEngine { inner, last_profile: None })),
        Err(_) => ptr::null_mut(),
    }
}

/// Free an engine created by `vc_engine_new*`. Passing null is a no-op.
///
/// # Safety
/// `engine` must have come from this library and not been freed already.
#[no_mangle]
pub unsafe extern "C" fn vc_engine_free(engine: *mut VcEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

/// Process `in_len` input samples, writing cleaned 16 kHz mono samples into
/// `out` (capacity `out_cap`). The number actually written is stored in
/// `*written`. Returns [`VC_OK`] or a negative error; [`VC_ERR_BUFFER`] means
/// `out_cap` was too small (try a buffer ≥ in_len, which is always sufficient
/// because output is never longer than input after downsampling).
///
/// # Safety
/// Pointers must be valid for the given lengths for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn vc_engine_process(
    engine: *mut VcEngine,
    input: *const f32,
    in_len: usize,
    out: *mut f32,
    out_cap: usize,
    written: *mut usize,
) -> c_int {
    let Some(engine) = engine.as_mut() else { return VC_ERR_NULL };
    if input.is_null() || out.is_null() || written.is_null() {
        return VC_ERR_NULL;
    }
    let inp = slice::from_raw_parts(input, in_len);
    let produced = engine.inner.process(inp);
    if produced.len() > out_cap {
        return VC_ERR_BUFFER;
    }
    ptr::copy_nonoverlapping(produced.as_ptr(), out, produced.len());
    *written = produced.len();
    VC_OK
}

/// Begin enrollment. Feed your voice via [`vc_engine_process`] afterwards.
///
/// # Safety
/// `engine` must be a valid handle.
#[no_mangle]
pub unsafe extern "C" fn vc_engine_begin_enrollment(engine: *mut VcEngine) -> c_int {
    let Some(engine) = engine.as_mut() else { return VC_ERR_NULL };
    engine.inner.begin_enrollment();
    VC_OK
}

/// Number of enrollment frames captured so far.
///
/// # Safety
/// `engine` must be a valid handle.
#[no_mangle]
pub unsafe extern "C" fn vc_engine_enrollment_frames(engine: *const VcEngine) -> u64 {
    match engine.as_ref() {
        Some(e) => e.inner.enrollment_frames(),
        None => 0,
    }
}

/// Finish enrollment, install the profile, and copy its serialised bytes into
/// `profile` (capacity `profile_cap`); the length is stored in `*plen`.
/// Returns [`VC_OK`], [`VC_ERR_STATE`] if not enrolling, or [`VC_ERR_BUFFER`].
/// Call with `profile = null` / `profile_cap = 0` first to learn the size.
///
/// # Safety
/// Pointers must be valid for their lengths.
#[no_mangle]
pub unsafe extern "C" fn vc_engine_finish_enrollment(
    engine: *mut VcEngine,
    profile: *mut u8,
    profile_cap: usize,
    plen: *mut usize,
) -> c_int {
    let Some(engine) = engine.as_mut() else { return VC_ERR_NULL };
    if plen.is_null() {
        return VC_ERR_NULL;
    }
    // Finish once and cache the bytes, so a size-query followed by a fetch both
    // see the same profile.
    if let Some(p) = engine.inner.finish_enrollment() {
        engine.last_profile = Some(p.to_bytes());
    }
    let Some(bytes) = engine.last_profile.as_ref() else { return VC_ERR_STATE };
    *plen = bytes.len();
    if profile.is_null() || profile_cap == 0 {
        // size query
        return VC_OK;
    }
    if bytes.len() > profile_cap {
        return VC_ERR_BUFFER;
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), profile, bytes.len());
    VC_OK
}

/// Install a previously saved profile blob (from a prior enrollment).
///
/// # Safety
/// `profile` must point to `len` valid bytes.
#[no_mangle]
pub unsafe extern "C" fn vc_engine_set_profile(
    engine: *mut VcEngine,
    profile: *const u8,
    len: usize,
) -> c_int {
    let Some(engine) = engine.as_mut() else { return VC_ERR_NULL };
    if profile.is_null() {
        return VC_ERR_NULL;
    }
    let bytes = slice::from_raw_parts(profile, len);
    match SpeakerProfile::from_bytes(bytes) {
        Some(p) => {
            engine.inner.set_profile(&p);
            VC_OK
        }
        None => VC_ERR_STATE,
    }
}

/// 1 if a profile is active, 0 otherwise.
///
/// # Safety
/// `engine` must be a valid handle.
#[no_mangle]
pub unsafe extern "C" fn vc_engine_is_enrolled(engine: *const VcEngine) -> c_int {
    match engine.as_ref() {
        Some(e) => e.inner.is_enrolled() as c_int,
        None => 0,
    }
}

/// The engine's fixed output sample rate (16000).
#[no_mangle]
pub extern "C" fn vc_output_sample_rate() -> u32 {
    verballi::OUTPUT_SAMPLE_RATE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_roundtrip() {
        unsafe {
            let e = vc_engine_new(16_000);
            assert!(!e.is_null());

            assert_eq!(vc_engine_begin_enrollment(e), VC_OK);
            let input = vec![0.1f32; 32_000];
            let mut out = vec![0.0f32; 32_000];
            let mut written = 0usize;
            assert_eq!(
                vc_engine_process(e, input.as_ptr(), input.len(), out.as_mut_ptr(), out.len(), &mut written),
                VC_OK
            );
            assert!(vc_engine_enrollment_frames(e) > 0);

            // Size query, then real fetch: the cached profile makes both succeed.
            let mut plen = 0usize;
            assert_eq!(vc_engine_finish_enrollment(e, ptr::null_mut(), 0, &mut plen), VC_OK);
            assert!(plen > 0);
            let mut profile = vec![0u8; plen];
            assert_eq!(
                vc_engine_finish_enrollment(e, profile.as_mut_ptr(), profile.len(), &mut plen),
                VC_OK
            );
            assert_eq!(vc_engine_is_enrolled(e), 1);

            // A buffer that's too small reports VC_ERR_BUFFER.
            let mut tiny = vec![0u8; plen - 1];
            let mut p2 = 0usize;
            assert_eq!(
                vc_engine_finish_enrollment(e, tiny.as_mut_ptr(), tiny.len(), &mut p2),
                VC_ERR_BUFFER
            );

            // The captured bytes can be re-installed on a fresh engine.
            let e2 = vc_engine_new(16_000);
            assert_eq!(vc_engine_set_profile(e2, profile.as_ptr(), profile.len()), VC_OK);
            assert_eq!(vc_engine_is_enrolled(e2), 1);

            vc_engine_free(e2);
            vc_engine_free(e);
        }
    }

    #[test]
    fn null_engine_is_safe() {
        unsafe {
            assert_eq!(vc_engine_begin_enrollment(ptr::null_mut()), VC_ERR_NULL);
            vc_engine_free(ptr::null_mut()); // no-op
        }
    }
}
