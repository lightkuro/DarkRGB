# Roadmap

Written on 2026-11-10. Order: audio-reactive first, then live control, then the timeline, then yard-scale
features. The design targets room/desk setups (a few WLED devices, up to a few thousand pixels) and house/yard
shows (many controllers, 5k to 50k pixels). See [architecture.md](architecture.md) for the design.

Version numbers go up with each round of work and are not tied to the stages below.

## 0. Checks that need real hardware

The plan rests on a few assumptions. Each has a check; `darkrgb-cli` runs the first three.

| Check | Done when | Status on 2026-11-10 |
|---|---|---|
| System audio on Linux | Live levels from whatever is playing, with no setup, following the default output | Passed in a container with virtual outputs, on PulseAudio 16.1 and PipeWire 1.0.5: all audio received, a burst detected within about 30 ms, default-output changes followed. Not yet run on a desktop with real speakers. |
| System audio on Windows | The same, through WASAPI loopback | **Passed** on the owner's Windows PC: `audio-test` reported 100% of the audio received. One machine so far. |
| DDP to a real WLED | 60 fps to a physical ESP32 for 10 minutes without visible stutter, on WiFi and on Ethernet | Passed against the simulator (60 fps, under 1 ms jitter on one machine). **Not run on hardware.** `darkrgb-cli ddp-test IP` |
| The app window on Windows | It opens, shows the UI and closes cleanly | It builds in CI. **Not run by anyone yet.** |
| Preview speed in the Linux app window | 10,000 points at 30 fps or better in WebKitGTK | The window works and draws the preview under a virtual display with software rendering. Speed on a real graphics card is **not measured.** |
| Key handling | Key down and up arrive reliably in the app window, several keys held | **Not run.** Belongs to live mode. |

If the preview is too slow in the Linux window, `darkrgb-cli` (browser) is the fallback today, and Electron on
Linux the longer-term one.

## 1. Audio-reactive

Built:

- One app window (Tauri) with the engine inside; `darkrgb-cli` for terminals, browsers and the checks.
- Finds WLED devices on the network and reads their LED count and matrix size; adds by address with the
  network's prefix filled in; scans a network on request.
- Demo lights and virtual lights that exist only in the preview.
- Fixtures placed on a 2D canvas by dragging, with rotate, resize and reverse.
- Ten effects in layers with four blend modes, each on all lights or chosen ones. Layers collapse, reorder by
  dragging, and can be renamed and duplicated.
- Each layer hears everything, bass, mids, highs or a custom range, with its own sensitivity, and its
  brightness can follow the sound.
- Simple by default: parameters are marked basic or advanced, and an Advanced switch reveals the rest.
- Live preview from the same frames the lights get.
- Zero-setup audio on Linux and Windows.
- Scenes, master brightness, blackout. The show saves itself.
- A simulated WLED device, used by the end-to-end tests.

Left before this stage is done:

- Run the hardware checks above and fix what they find.
- Tune the effects and the audio levelling against real music and real LEDs. So far they have only met
  synthetic test songs. `cargo run --release -p darkrgb-audio --example spread` prints how much each audio feature
  moves, which is the number to watch.
- Link any slider to the sound, the beat or a slow wave ("modulators"). Today only brightness can follow sound.
- Custom light shapes (a drawn line, a ring, a list of points) for virtual and real lights.
- Turn a virtual light into a real one by pointing it at a device, keeping its place and effects.
- Installers and a signed Windows build. Today the download is a zip of two programs.
- Warn when a WLED setting fights the stream (its own brightness or gamma on realtime data).
- Rename fixtures in the UI (devices can be renamed); split one device into several fixtures.
- Undo.

Done when a new user can install the app, see their WLED devices, and have them reacting to music in under two
minutes.

## 2. Live mode

- Grid of clips and scenes.
- Keyboard and MIDI mapping with a "learn" button; flash, toggle and hold behaviours.
- Tap tempo, and beat-quantised launching.
- Master dimmer, blackout, colour palettes.
- Layers with blend modes.

Done when a full song can be performed from the keyboard without touching the mouse.

## 3. Timeline

- Import a song; show its waveform and an automatic beat grid.
- Clips on layers with trim, move and snap to beats.
- Curves on parameters.
- Record a live performance into the timeline.
- Undo and redo everywhere.

Done when a three-minute show can be recorded live, cleaned up in the editor and played back in sync.

## 4. Yard scale

- sACN and Art-Net controllers.
- Import an xLights layout.
- Export `.fseq` v2 and upload to FPP.
- Scheduler for unattended playback.
- 3D preview.
- Performance target: 50,000 pixels at 40 fps on a mid-range PC. Measured on 2026-11-10 for the engine alone:
  50,000 pixels on ten simulated devices with three effect layers ran at 60 fps, 4.2 ms of work per frame, on a
  4-core cloud machine. Real networks and the preview at that size are not measured.

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
| Name | Decided 2026-11-10: darkRGB | The first name, OpenLightShow, clashed with an existing Python app of the same name and kind ([digidigital/openlightshow](https://github.com/digidigital/openlightshow), on PyPI as `openlightshow`). For darkRGB no GitHub repo, user or organisation of that name was found, `darkrgb` was free on crates.io, npm and PyPI, and web searches found no product called that. This is not a trademark search. Names ending in "RGB" usually mean PC keyboard and fan lighting, so some people will assume that first. |
| Linux shell | Tauri (WebKitGTK) or Electron | Tauri is built and works. Electron only if the preview proves too slow on real hardware. |
| Phone remote | Password or pairing code for `--lan` | Needed before recommending `--lan`. Today anyone on the network can control the lights. |
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
