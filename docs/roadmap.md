# Roadmap

Written on 2026-11-10, last updated 2026-11-12 for version 0.7.0. Order: audio-reactive first, then live control, then the timeline, then yard-scale
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
| The app window on Windows | It opens, shows the UI and closes cleanly | 0.2.0 built in CI. **Not run by anyone yet.** |
| Preview speed in the Linux app window | 10,000 points at 30 fps or better in WebKitGTK | The window works and draws the preview under a virtual display with software rendering. Speed on a real graphics card is **not measured.** |
| Key handling | Key down and up arrive reliably in the app window, several keys held | **Passed on Linux** in the app window (WebKitGTK under a virtual display): a held key, a second key pressed while it was held, taps and number keys all arrived. Windows not run. |
| 120 frames a second | A real strip visibly smoother at 120 than at 60, with no dropped packets | The engine holds 120 with time to spare (0.05 ms of work a frame for the demo lights). **Not run on hardware.** The LEDs set the ceiling: about 110 a second for 300 WS2812 LEDs on one output, 33 for 1,000. |
| Song energy and sections | On real music, quiet parts read low, and drops are caught within a second or two without false alarms | Passed in unit tests on synthetic songs (steady; breakdown then drop; quiet intro then drop; small volume changes) and on one synthetic song played through the real audio path, where both drops and the breakdown were heard within 2 seconds. **Not tried on real music.** |

If the preview is too slow in the Linux window, `darkrgb-cli` (browser) is the fallback today, and Electron on
Linux the longer-term one.

## 1. Audio-reactive

Built:

- One app window (Tauri) with the engine inside; `darkrgb-cli` for terminals, browsers and the checks.
- Finds WLED devices on the network and reads their LED count and matrix size; adds by address with the
  network's prefix filled in; scans a network on request.
- Demo lights and virtual lights that exist only in the preview.
- Fixtures placed on a 2D canvas by dragging, with rotate, resize and reverse.
- Twenty effects in layers with four blend modes, each on all lights, chosen ones, or only strips or only
  matrices, drawn across them or once per light. Layers collapse, reorder by dragging, and can be renamed and duplicated.
- The song's shape: how big this part of the song is against the last 45 seconds, and when it moves to a new
  section. Quiet parts are dimmed and slowed by an amount the user sets.
- Fifteen starter scenes, placed on strips and matrices as suits each, including on lights added later.
- Three kinds of show (a room show, a light show, a live show) and blank. The kind chosen when a show is started sets
  its scenes, pads, how scenes change and how it treats flashing.
- Any slider can follow loudness, bass, mids, highs, the beat, hits, the song's energy, a slow wave in time
  with the music, or a fader in the Live view.
- Palettes of the user's own, two to eight colours, carried inside the setting that uses them.
- Effects, looks and palettes kept in the library to use in any show.
- A scene to wait in when the music stops, with a fade there and back.
- Lights picked on the stage one or several at a time (click, Shift and click, box, whole group), moved and
  turned together, with a typed angle.
- A flash limit on the final picture, kept with each show. The show asks once, the first time something
  flashes fast. Strobes slow themselves to the limit; an effect that only works flashing fast is switched off
  while it is on.
- 120 frames a second as a choice, with each light's own ceiling shown next to it.
- Each layer hears everything, bass, mids, highs or a custom range, with its own sensitivity, and its
  brightness can follow the sound.
- Folders for effects (switch, dim, fold, drag in and out) and groups of lights (an effect can play on a
  group; a group dims or switches as one).
- A settings window: brightness ceiling, frames per second, the flash limit, a delay for late speakers, how
  much to follow the song's energy, autopilot, keys.
- A library of shows: list, open, new (keeping the lights or not), copy, rename, delete to a trash folder,
  bring in from a file, save a copy.
- A simple layout by default and a full one, switched by one setting kept on each device. Parameters are
  marked basic or advanced; the simple layout also leaves out folders, groups, mixing, keys on effects and
  the less common settings.
- Live preview from the same frames the lights get.
- Zero-setup audio on Linux and Windows.
- Scenes, master brightness, blackout. The show saves itself.
- A simulated WLED device, used by the end-to-end tests.

Left before this stage is done:

- Run the hardware checks above and fix what they find.
- Tune the effects, the audio levelling, the song-energy tracking and the drop detection against real music
  and real LEDs. So far they have only met synthetic test songs. `cargo run --release -p darkrgb-audio --example spread` prints how much each audio feature
  moves, which is the number to watch.
- Custom light shapes (a drawn line, a ring, a list of points) for virtual and real lights.
- Turn a virtual light into a real one by pointing it at a device, keeping its place and effects.
- Installers and a signed Windows build. Today the download is a zip of two programs.
- Warn when a WLED setting fights the stream (its own brightness or gamma on realtime data).
- Rename fixtures in the UI (devices can be renamed); split one device into several fixtures.
- Undo.

Done when a new user can install the app, see their WLED devices, and have them reacting to music in under two
minutes.

## 2. Live mode

Built:

- Pads tied to scenes, effects, folders of effects, groups of lights, blackout, tap tempo, a hand-fired beat,
  next scene, previous scene and freeze. A pad may have a key, assigned by pressing it ("learn"), or none.
- Three things a press can do: switch on each press, act only while held, or fire the effect's reaction as if
  a beat had hit. A scene's key can show the scene only while held. An effect can be set to ignore the music
  and react only when fired.
- A Live view: the stage and every pad, usable by mouse and touch. Pads are set up in place: key, behaviour,
  order, removal, and adding a pad for anything that has none.
- Ready-made pads (white flash, strobe, colour pulse) in a pinned folder that scene changes leave alone.
- Faders for the speed of everything and for sound sensitivity.
- Tap tempo, which replaces the tempo found in the music until told to follow the music again.
- Scene changes at once, on the next beat or on every fourth beat, with an optional fade.
- Autopilot: the next scene every 2, 4, 8 or 16 bars and at once on a change of section, in order or
  shuffled. A scene can be left out, and can say whether it suits the quiet parts of a song or the big ones;
  autopilot chooses accordingly. Pressing a scene restarts the count.
- Four faders that sliders can follow, shown in the Live view when something does.
- A key held by a window that closes or loses focus is let go.
- Master dimmer, blackout, palettes, layers with blend modes (from stage 1).
- MIDI controllers, with the same "learn" as keys: notes on pads, knobs and faders on brightness, speed,
  sensitivity and the four faders. The engine listens for MIDI itself, so it works in the app window on
  Linux, where the web view has no MIDI. **Not yet tried with a real controller**, and the machine it was
  built on cannot even make a pretend one, so only the handling of messages is tested.
- Colour pads: one press changes the colours of everything that is playing.
- Four pages of pads, and an XY pad that bends two faders at once.
- Undo and redo for every change to a show.
- A phone or tablet as a remote, let in by a six-digit code shown on the computer.

Left:

- A grid of clips: today a pad switches or fires an effect that is already in the stack. Clips that start from
  their beginning when launched, and one-shot clips that end by themselves, are not built.
- Seeing which key does what on one printable page.
- Lights on a controller's pads showing what is on (MIDI feedback).

Done when a full song can be performed from the keyboard without touching the mouse.

## 3. Timeline

Built in 0.7.0:

- A song chosen from a file, kept by the engine, played by the app window; its outline drawn, and a tempo
  and first beat guessed from it for a grid.
- A lane for scenes and a lane for every pad. Clips are placed, moved, stretched, picked several at a time
  and deleted, and snap to beats, halves or quarters.
- Recording: scenes chosen and pads pressed while the timeline plays become clips.
- Play, stop, jump, repeat. The engine works out what should be on from the clips alone each frame, so a
  jump or a repeat needs nothing special.
- The lights follow the song: the window playing it tells the engine where it is.

Left:

- Curves on settings (a slider drawn over time). Today a timeline can only say which scene shows and which
  pads are held.
- Seeing the look at the playhead while stopped. Today the timeline acts only while it plays.
- Copy, paste and repeat of clips; a split tool; markers; more than one song in a show; a playlist.
- Playing the song without the app window open (the engine does not decode sound itself), and so running a
  timeline from a schedule with nobody there.
- Exporting to `.fseq` for a Falcon Player.
- The song's own start is where a person lines it up: there is no fine offset between song and lights
  beyond the show's light delay.

Tried with a 31-second synthetic song in a browser and in the app window on Linux. Not yet tried with real
music, a long song, MP3 on a Linux system without the codecs, or on Windows.

Done when a three-minute show can be recorded live, cleaned up in the editor and played back in sync.

## 4. Yard scale

Built:

- sACN (E1.31) and Art-Net controllers, sent straight to an address (unicast), 170 LEDs to a universe, RGB
  order. **Not yet tried with a real controller.** Multicast sACN, other colour orders, and a controller's
  outputs starting mid-universe are not built.
- Hours for a show to keep each day. A calendar (different days, dates, sunset) is not built.
- Words and small pictures on matrices. Animated pictures are not built.

Left:

- Strips bent into shapes (0.7.0 has arches, rings, stars, zigzags and hand-drawn paths; not yet matrices of
  odd shapes, or a tree as one model).
- Import an xLights layout.
- Export `.fseq` v2 and upload to FPP.
- 3D preview.
- Performance target: 50,000 pixels at 40 fps on a mid-range PC. Measured on 2026-11-10 for the engine alone:
  50,000 pixels on ten simulated devices with three effect layers ran at 60 fps, 4.2 ms of work per frame, on a
  4-core cloud machine. Real networks and the preview at that size are not measured.

## 5. Later

- User effects as shaders or WASM plugins.
- DMX fixtures (pars, moving heads).
- Phone remote polish: a layout made for a phone, and the remote switched on without a restart.
- Ableton Link tempo sync.
- Hue, OpenRGB and other device families.
- macOS build.

## An outside review of 0.6.0

On 2026-11-12 an independent reviewer was given the repository and asked what keeps this from being a truly
good audio-reactive and live tool. It could read everything and run nothing. Its ranked findings, and where
each stands after 0.7.0:

1. **The beat clock.** Lights are always a little late and nothing fires early to make up for it; bars are
   counted from whenever listening began, not from beat one; when the music breaks down the tempo is lost
   and every onset becomes a "beat"; the tracker can lock at double time; tap tempo replaces the music's
   tempo and then drifts; there is no nudge, no halving or doubling, no beat display, no MIDI clock or
   Ableton Link. **Not addressed. This is the first job of the next round.**
2. **Effects and tempo.** Most speeds were in seconds, not beats; no mirror, repeat or kaleidoscope for any
   effect; links have one wave shape and no smoothing; colour cannot be modulated; no pitch-to-colour; the
   section detector judges by loudness alone and is reset by a short silence. *Partly addressed:* seven of
   the eight new effects count in beats, and scanner, theatre chase (Marquee), breathe and falling peaks
   were among the staples it named. The rest stands.
3. **Nothing has met real hardware or real music.** Stands, and only the owner can change it.
4. **The outdoor display audience was promised a timeline that did not exist,** and shapes were only straight
   strips and rectangles. *Addressed in part:* 0.7.0 has a timeline and bent strips. Still missing: xLights
   import, FSEQ export, a calendar, power limiting by current, colour orders and start channels for sACN.
5. **The live model is shallow next to a console.** A scene replaces everything; no cue list; MIDI ignores
   velocity and gives no feedback; no DMX fixtures. Stands.
6. **Output under load.** All sends are on the render thread through one socket, and a full buffer drops the
   rest of a device's universes for that frame; no sACN or Art-Net sync; no latency offset for each device;
   8-bit output with no dithering, so slow dim fades will step. Stands.
7. **Unattended reliability.** A panic in an effect would stop the show silently; no autostart; no
   stream-terminated packet on exit; the flash limit judges the whole picture. Stands.

It also advised: stop adding features until a real device and real music have been through the loop; call
autopilot's "bars" what they are (four beats from an arbitrary start) until there is a downbeat; reconsider
dimming quiet parts by default for live shows.

## Before the first public release

The repository is public. Done since it was first listed here: the licences of the bundled libraries ship
with every download (`tools/third_party.py` makes the list for each build), a Release workflow drafts a
GitHub release with installers and checksums, other devices need a pairing code, and the repository is named
after the product.

Still to do before the first release is published:

- Run the Release workflow once and try what it makes: the Windows setup program has never been built, and
  none of the installers has been installed on a real desktop.
- Merge `dev` into `main`, which is still the first version.
- Try real hardware: a WLED device, a MIDI controller, and an sACN or Art-Net controller if one is to hand.
- Decide about signing. Unsigned, Windows shows a warning before the setup program runs.

## Open decisions

| Decision | Options | Recommendation |
|---|---|---|
| Licence | Decided 2026-11-10: GPL-3.0-or-later | xLights, LedFx and the aubio beat library are GPL-3, so this allows porting their effects. |
| Name | Decided 2026-11-10: darkRGB | The first name, OpenLightShow, clashed with an existing Python app of the same name and kind ([digidigital/openlightshow](https://github.com/digidigital/openlightshow), on PyPI as `openlightshow`). For darkRGB no GitHub repo, user or organisation of that name was found, `darkrgb` was free on crates.io, npm and PyPI, and web searches found no product called that. This is not a trademark search. Names ending in "RGB" usually mean PC keyboard and fan lighting, so some people will assume that first. |
| Linux shell | Tauri (WebKitGTK) or Electron | Tauri is built and works. Electron only if the preview proves too slow on real hardware. |
| Phone remote | Password or pairing code for `--lan` | Needed before recommending `--lan`. Today anyone on the network can control the lights. The Live view's pads already work by touch. |
| Where settings live | Decided 2026-11-11, changed 2026-11-12 | The frame rate, sound source, sensitivity, speaker delay, energy setting and brightness ceiling describe the room and the computer. (The flash limit was among them until 0.5.0, when it moved to the show: it depends on who will see the lights.) They are still written into each show file, but opening, starting or importing a show keeps the ones in use, so a show file never changes them. |
| Flash limit default | Decided 2026-11-11: on, "hard flashes" level; since 2026-11-12 asked once per show, and off for live shows | Counts a flash when the average brightness of the whole picture jumps by a quarter of full or more, and allows three a second. The stricter level counts a jump of a tenth, the threshold used for screens; at that level it softened most beat-driven looks, so it is offered as a choice and is not the default. One switch turns the limit off. |
| Windows code signing | Sign or ship unsigned | Unsigned installers trigger SmartScreen warnings; certificates cost money. Ship unsigned until there are users. |

## Things to keep in mind

- **Scope:** this is three products in one. Each milestone must be usable on its own, and the effect list stays
  short until the engine is solid.
- **Safety:** the flash limit, and a question asked once per show about it, are built. The limit looks at the
  whole picture, so one small light flashing fast among many steady ones is not held back. Still to do: cap
  brightness or power per device so a mistake cannot overload a power supply.
- **Network exposure:** the engine listens on localhost only by default. With phone access on it listens on
  the network, asks other devices for a code, and speaks plain HTTP; see SECURITY.md. The first run on
  Windows triggers a firewall prompt for device discovery; explain it in the app.
- **Latency:** show the measured audio-to-light delay and give a per-device offset.
- **Linux packaging:** a `.deb` and an AppImage are built. Flatpak is not; it needs network and audio
  permissions declared.
- **Testing:** golden-frame tests for effects (same inputs give the same pixels) and the simulated WLED device
  in CI.
- **Sharing shows:** songs are copyrighted, so exported or shared shows reference the audio and do not bundle it.
