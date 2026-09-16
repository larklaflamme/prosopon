# Level Meter (dB grid + waveform)

## Purpose
Replace the static "Say Hey Skye to begin" transcript panel with a live
two-channel audio level meter, giving visual feedback for both the user
and the agent during conversation.

## Design
- Two stacked channels: "you" (mic input) and "sky" (TTS playback).
- Scrolling level-over-time plot (continuous waveform, filled area).
- dB grid: horizontal lines every 20 dB (0 to -80), labeled on the left.
- Color zones per segment: yellow (too quiet), green (sweet spot),
  red (too loud).
- Live dB readout in the top-right of each channel.

## Scale
Mic RMS is tiny (background ~0.0001, speech ~0.006), invisible on a linear
scale. Converted to dBFS (20*log10(rms)) over -80..0 dB so speech sits
~45-67% up the grid and TTS (near full scale) tops out.

## Sources
- "you": live mic RMS tapped from the mic bus.
- "sky": live playback RMS from a new source wrapper in client-core.

## Known behavior
- The agent channel reads red while Skye speaks (TTS is normalized near
  full scale, always above the "too loud" threshold). Expected, not a bug.
- dB thresholds are linear guesses (0.005 / 0.05); tune from live photos.

## Files
- client-core/src/playback.rs  (playback RMS source wrapper)
- src-tauri/src/lib.rs         (level event wiring)
- ui/index.html                (panel removed, meter added)
- ui/main.js                   (level handler)
- ui/styles.css                (meter styling)
- ui/avatar.js                 (animation + meter)
