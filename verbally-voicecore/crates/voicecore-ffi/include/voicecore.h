/* voicecore.h — C ABI for the voicecore real-time voice-isolation engine.
 *
 * Link against libvoicecore_ffi (staticlib for Apple, cdylib for Android/desktop).
 * All buffers are caller-owned; the library never retains pointers past a call.
 * Functions return VC_OK (0) or a negative VC_ERR_* code.
 *
 * Output is always 16 kHz mono f32 — feed it straight into an ElevenLabs
 * speech-to-speech / agent stream.
 */
#ifndef VOICECORE_H
#define VOICECORE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VcEngine VcEngine;

#define VC_OK          0
#define VC_ERR_NULL   -1
#define VC_ERR_CONFIG -2
#define VC_ERR_BUFFER -3
#define VC_ERR_STATE  -4

/* Create / destroy. Returns NULL on invalid configuration. */
VcEngine *vc_engine_new(uint32_t input_sample_rate);
VcEngine *vc_engine_new_tuned(uint32_t input_sample_rate,
                              float highpass_hz,
                              float denoise_strength,
                              float speaker_focus,
                              float proximity_focus);
void vc_engine_free(VcEngine *engine);

/* Process input (at input_sample_rate) -> cleaned 16 kHz mono into `out`.
 * A buffer of capacity >= in_len is always sufficient. */
int vc_engine_process(VcEngine *engine,
                      const float *input, size_t in_len,
                      float *out, size_t out_cap, size_t *written);

/* Enrollment: begin, feed your voice via vc_engine_process, then finish.
 * Call finish with profile=NULL, profile_cap=0 first to learn *plen. */
int vc_engine_begin_enrollment(VcEngine *engine);
uint64_t vc_engine_enrollment_frames(const VcEngine *engine);
int vc_engine_finish_enrollment(VcEngine *engine,
                                uint8_t *profile, size_t profile_cap, size_t *plen);

/* Persisted profiles. */
int vc_engine_set_profile(VcEngine *engine, const uint8_t *profile, size_t len);
int vc_engine_is_enrolled(const VcEngine *engine);

/* Always returns 16000. */
uint32_t vc_output_sample_rate(void);

#ifdef __cplusplus
}
#endif

#endif /* VOICECORE_H */
