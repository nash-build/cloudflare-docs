# Clean-room notes

The goal was to build a noise/voice isolation engine **without stepping on the
IP of any SDK** you may have been studying. Here's how this codebase stays clear.

## What we did

- **Wrote everything from first principles.** Every algorithm here —
  resampling, biquad high-pass, STFT/WOLA, Wiener suppression, VAD, AGC, the
  band-energy speaker fingerprint, the proximity heuristic — is a textbook DSP/ML
  technique implemented from scratch. None of it is copied or transcribed from
  another product's source.
- **Used only permissively-licensed building blocks.** The single runtime
  dependency in the core is `rustfft` (MIT/Apache). Suggested upgrades
  (DeepFilterNet, ECAPA-TDNN/ONNX via `ort`) are also permissively licensed; cite
  and comply with their licenses if you adopt them.
- **Kept ElevenLabs at arm's length.** We don't bundle, fork, or modify the
  ElevenLabs SDK. The engine emits standard PCM; the connection uses ElevenLabs'
  own published SDK/protocol, unchanged, in the app shell.

## What to avoid

- **Don't copy code or assets** out of a proprietary/closed SDK (source,
  model weights, lookup tables, coefficients) into this project.
- **Don't reverse-engineer in violation of a license.** If any SDK you were
  examining has terms forbidding reverse engineering, use it only as a black box
  and rely on the open techniques here instead.
- **Re-implementing a documented *concept*** (e.g. "noise suppression",
  "target-speaker extraction") is fine; **reproducing someone's specific
  implementation** is not.

## If you send the reference SDKs

They can inform *understanding* (what features matter, what the audio chain looks
like) and we can cross-check the proximity approach against them — but the code in
this repo stays independently written. Flag any SDK whose license restricts this
and we'll route around it.

> This is engineering guidance, not legal advice. For anything you intend to ship
> commercially, have counsel review the licenses of every dependency and of any
> SDK you studied.
