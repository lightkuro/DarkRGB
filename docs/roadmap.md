# Roadmap

Written on 2026-11-10. Order: audio-reactive first, then live control, then the timeline, then yard-scale
features. The design targets room/desk setups (a few WLED devices, up to a few thousand pixels) and house/yard
shows (many controllers, 5k to 50k pixels). See [architecture.md](architecture.md) for the design.

## 0. Checks that need real hardware

The plan rests on a few assumptions. Each has a check; the app's own diagnostics run the first three.

| Check | Done when | Status on 2026-11-10 |
|---|---|---|
| System audio on Linux | Live levels from whatever is playing, with no setup, following the default output | Passed in a container with virtual outputs, on PulseAudio 16.1 and PipeWire 1.0.5: all audio received, a burst detected within about 30 ms, default-output changes followed. Not yet run on a desktop with real speakers. |
| System audio on Windows | The same, through WASAPI loopback | Code written and compiles for Windows. **Not run.** `openlightshow audio-test` |
| DDP to a real WLED | 60 fps to a physical ESP32 for 10 minutes without visible stutter, on WiFi and on Ethernet | Passed against the simulator (60 fps, under 1 ms jitter on one machine). **Not run on hardware.** `openlightshow ddp-test IP` |
| Preview speed in WebKitGTK | 10,000 points at 30 fps or better in a Linux webview | **Not run.** Chromium shows 466 points at 60 fps with software rendering. |
| Key handling | Key down and up arrive reliably in the webview, several keys held | **Not run.** Belongs to live mode. |

If the preview check fails on Linux, the Linux shell becomes Electron. If it fails everywhere, fall back to a
pure-Rust UI.

## 1. Audio-reactive MVP (version 0.1)

Built:

- Finds WLED devices on the network and reads their LED count and matrix size; add by address as a fallback.
- Fixtures placed on a 2D canvas by dragging, with rotate, resize and reverse.
- Ten effects with parameters, in layers with four blend modes, each on all lights or chosen ones.
- Live preview from the same frames the lights get.
- Zero-setup audio on Linux (Windows written, untested).
- Scenes, master brightness, blackout. The show saves itself.
- A simulated WLED device, used by the end-to-end tests.

Left before 0.1 is done:

- Run the hardware checks above and fix what they find.
- Tune the effects and the audio levelling against real music and real LEDs. So far they have only met a
  synthetic test song.
- Desktop window (Tauri) and installers. Today the UI opens in the browser.
- Warn when a WLED setting fights the stream (its own brightness or gamma on realtime data).
- Rename devices and fixtures in the UI; split one device into several fixtures.
- Undo.

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
| Licence | Decided 2026-11-10: GPL-3.0-or-later | xLights, LedFx and the aubio beat library are GPL-3, so this allows porting their effects. |
| Name | Keep "OpenLightShow" or rename | **Rename before going public.** An MIT-licensed Python app with the same name exists: [digidigital/openlightshow](https://github.com/digidigital/openlightshow), "a music-reactive beamer lightshow application", published on PyPI as `openlightshow` (1.0.3, January 2026). Same name, same kind of product. It is small (3 stars on 2026-11-10), and no trademark registration turned up in a web search, which is not a legal check. The names on crates.io and npm are free. |
| Linux shell | Tauri (WebKitGTK) or Electron | Decided by the preview check above. |
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
