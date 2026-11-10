# Roadmap

Written on 2026-11-10. Order: audio-reactive first, then live control, then the timeline, then yard-scale
features. The design targets room/desk setups (a few WLED devices, up to a few thousand pixels) and house/yard
shows (many controllers, 5k to 50k pixels). See [architecture.md](architecture.md) for the design.

## 0. Risk spikes

Small throwaway programs that test the assumptions the whole plan rests on. Do these before building anything
real.

| Spike | Done when |
|---|---|
| System audio capture on Windows | A Rust program prints live volume from whatever Windows is playing, with no user setup, and survives switching the default output device |
| System audio capture on Linux | The same on PipeWire and on PulseAudio, picking the default sink's monitor automatically |
| DDP to a real WLED | 60 fps to a physical ESP32 for 10 minutes, with frame timing jitter and drops measured, on WiFi and on Ethernet |
| Preview throughput | A Tauri window on Linux (WebKitGTK) draws 10,000 points from WebSocket frames at 30 fps or better |
| Key handling | Key down and key up arrive reliably and quickly in the webview on both OSes, including several held keys |

If the preview spike fails on Linux, switch the Linux shell to Electron. If it fails everywhere, fall back to
the pure-Rust UI.

## 1. Audio-reactive MVP (version 0.1)

- Find WLED devices on the network and read their LED count and matrix size.
- Place fixtures on a 2D canvas.
- About 10 effects, each with a few parameters, driven by audio.
- Live preview that matches the real lights.
- Audio works with no setup on Windows and Linux.
- Save and recall scenes.
- A simulated WLED device for testing without hardware.

Done when a new user can install the app, see their WLED devices, and have them reacting to music in under two
minutes.

## 2. Live mode (0.2)

- Grid of clips and scenes.
- Keyboard and MIDI mapping with a "learn" button; flash, toggle and hold behaviours.
- Tap tempo, and beat-quantised launching.
- Master dimmer, blackout, colour palettes.
- Layers with blend modes.

Done when a full song can be performed from the keyboard without touching the mouse.

## 3. Timeline (0.3)

- Import a song; show its waveform and an automatic beat grid.
- Clips on layers with trim, move and snap to beats.
- Curves on parameters.
- Record a live performance into the timeline.
- Undo and redo everywhere.

Done when a three-minute show can be recorded live, cleaned up in the editor and played back in sync.

## 4. Yard scale (0.4)

- sACN and Art-Net controllers.
- Import an xLights layout.
- Export `.fseq` v2 and upload to FPP.
- Scheduler for unattended playback.
- 3D preview.
- Performance target: 50,000 pixels at 40 fps on a mid-range PC.

## 5. Later

- User effects as shaders or WASM plugins.
- DMX fixtures (pars, moving heads).
- Phone remote polish.
- Ableton Link tempo sync.
- Hue, OpenRGB and other device families.
- macOS build.

## Open decisions

| Decision | Options | Recommendation |
|---|---|---|
| Licence | GPL-3.0-or-later, or MIT/Apache-2.0 | GPL-3.0-or-later. xLights, LedFx and the aubio beat library are GPL-3, so GPL allows porting their effects. A permissive licence means writing every effect and the beat tracker from scratch. Decide before accepting outside contributions, since changing later needs every contributor's consent. |
| Name | Keep "OpenLightShow" or rename | Search for clashes (existing projects, trademarks) before publicising. Not checked yet. |
| Linux shell | Tauri (WebKitGTK) or Electron | Decided by the preview spike. |
| Windows code signing | Sign or ship unsigned | Unsigned installers trigger SmartScreen warnings; certificates cost money. Ship unsigned until there are users. |

## Things to keep in mind

- **Scope:** this is three products in one. Each milestone must be usable on its own, and the effect list stays
  short until the engine is solid.
- **Safety:** limit strobe rate and show a photosensitivity warning. Cap brightness or power per device so a
  mistake cannot overload a power supply.
- **Network exposure:** the engine listens on localhost only by default. The first run on Windows triggers a
  firewall prompt for device discovery; explain it in the app.
- **Latency:** show the measured audio-to-light delay and give a per-device offset.
- **Linux packaging:** AppImage and Flatpak. Flatpak needs network and audio permissions declared.
- **Testing:** golden-frame tests for effects (same inputs give the same pixels) and the simulated WLED device
  in CI.
- **Sharing shows:** songs are copyrighted, so exported or shared shows reference the audio and do not bundle it.
