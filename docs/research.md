# Research: existing LED show tools

Survey done on 2026-11-10 from project docs, source code and forums. Lines marked **(unverified)** come from
general knowledge and were not checked against a source in this survey.

## Summary

| Tool | Stack and licence | Best parts to copy | Pain points to avoid |
|---|---|---|---|
| xLights (+ FPP) | C++/wxWidgets, GPL-3, Windows/macOS/Linux | Models and groups, layer blending, value curves, timing tracks, `.fseq` and FPP ecosystem | Steep learning curve, dense UI, long renders, universe arithmetic, no live mode |
| Vixen 3 | C#/.NET, Windows only | Video-editor-like timeline, tap-in marks, smart-shape props | Windows only, slow above ~13k pixels, small community, no live mode |
| LedFx | Python + React, GPL-3 | Audio effects, virtuals spanning devices, scenes, WLED discovery | Audio setup (worst on Linux), latency sensitivity, no timeline |

**The gap:** no tool combines audio-reactive, live and timeline modes, and none has a modern, approachable UI.
The sequencers (xLights, Vixen) have no real-time mode; the real-time tool (LedFx) has no timeline and weak
manual control.

## xLights, xSchedule and Falcon Player (FPP)

**Facts**

- GPL-3.0, C++ with wxWidgets. Runs on Windows, macOS and Linux.
  [Repo](https://github.com/xLightsSequencer/xLights)
- Very active: release 2026.19 on 2026-10-23 added an SVG picture effect, guitar/piano stem separation and
  Light-O-Rama S5/S6 import. [xlights.org](https://xlights.org/)
- Layout is built from models, model groups and submodels. An `.xmodel` file can hold several models with their
  relative positions, and models can be downloaded from vendors.
  [Model import](https://manual.xlights.org/xlights/chapters/chapter-four-tabs/download-import-models.md)
- The sequencer puts effects on layers. Layers have a mix slider, 19 in/out transitions, and a canvas mode that
  lets an effect (warp, kaleidoscope) act on the layers below it.
  [Layer blending](https://manual.xlights.org/xlights/chapters/chapter-four-sequencer/layers/layer-blending.md)
- Value curves (flat, ramp, sawtooth, custom, exponential, logarithmic, parabolic) can drive effect parameters
  over time. [Value curves](https://manual.xlights.org/xlights/chapters/chapter-four-sequencer/value-curves.md)
- Timing tracks (beats, bars) are generated with Queen Mary Vamp plugins. Lyrics are split into phonemes for
  singing faces. [Singing faces](https://manual.xlights.org/xlights/chapters/chapter-four-sequencer/singing-faces)
- Outputs: E1.31/sACN, Art-Net, DDP, ZCPP (Falcon only), OPC.
  [Ethernet controllers](https://manual.xlights.org/xlights/chapters/chapter-four-set-up/lighting-networks/ethernet-controller.md)
- FPP is the usual playback device: a Raspberry Pi or BeagleBone that plays `.fseq` files and outputs E1.31, DDP,
  DMX and more. [FPP repo](https://github.com/FalconChristmas/fpp)
- xSchedule can be triggered by buttons, MIDI and Art-Net, but effects are rendered ahead of time. No real-time
  or audio-reactive mode was found.

**The `.fseq` v2 format** matters for compatibility with FPP-based displays. It is a pre-rendered file: one frame
of channel bytes per time step (usually 25 or 50 ms). The header starts with `PSEQ` and holds the channel count,
frame count and step time. Frames are compressed in blocks with zstd or zlib, and sparse ranges let a file hold
only some channels.
[Format spec](https://github.com/FalconChristmas/fpp/blob/master/docs/FSEQ_Sequence_File_Format.txt)

**Pain points**

- "Steeper learning curve, but far more creative power."
  [Forum](https://auschristmaslighting.com/threads/vixen-vs-xlights-midlife-crisis.12316/post-107693)
- Renders that take over an hour on shows above about a million channels.
  [Forum](https://auschristmaslighting.com/threads/13383)
- Dense, dated UI and manual universe/channel setup **(unverified, widely repeated)**.

## Vixen 3

**Facts**

- C# on .NET, Windows only. The README says "unlicensed and free to use" and points to a separate licence file,
  so the terms are unclear. [Repo](https://github.com/VixenLights/vixen)
- Active: 3.13 on 2026-08-12 moved to .NET 10 and added a universal live preview and direct upload to FPP.
  [Release notes](https://www.vixenlights.com/docs/release-notes/2026/vixen-3-13/)
- Preview with "smart shapes" (mega tree, arch, net), a custom prop editor and a moving-head fixture wizard.
  [Docs](https://www.vixenlights.com/docs/)
- Timeline with layers, curve and gradient editors, preset libraries, and mark collections that can be tapped in
  along with the music. Beat and bar detection uses Vamp plugins.
- A web interface and REST API can switch elements on and play sequences.
  [Web interface](https://www.vixenlights.com/docs/usage/web-interface/)

**Pain points**

- Lag above about 13k pixels and high memory use; "very few Vixen users left" (forum posts).
- Windows only. No real-time or audio-reactive mode.

## LedFx

**Facts**

- GPL-3.0. Python backend (aiohttp, numpy, PortAudio, aubio) with a React frontend served on port 8888.
  [README](https://github.com/LedFx/LedFx)
- Active: 2.2.1 on 2026-10-04. [Releases](https://github.com/LedFx/LedFx/releases)
- About 70 effect classes, including 2D matrix effects (equalizer, plasma, flame, GIF player, text).
  [Effects](https://github.com/LedFx/LedFx/tree/main/ledfx/effects)
- "Virtuals" map one effect across segments of several devices. Scenes save the whole state.
- Integrations: Spotify, QLC+, MQTT, Home Assistant, Launchpad/MIDI.
- Audio analysis: FFT through aubio, then mel bands from 20 Hz to 15 kHz split into low (to 350 Hz), mid (to
  2 kHz) and high. DDP is the default output for WLED.

**Audio setup, the main complaint**

- Windows: the code now ships a patched PortAudio that exposes WASAPI loopback devices and prefers the loopback
  of the default output. The official audio guide is stale and still tells users to enable Stereo Mix or install
  Voicemeeter, and a developer note describes loopback failing at startup when certain other audio drivers are
  present. [audio.py](https://github.com/LedFx/LedFx/blob/main/ledfx/effects/audio.py),
  [audio guide](https://github.com/LedFx/LedFx/blob/main/docs/settings/directing_audio.md)
- Linux: the docs tell users to pick the "pulse" device and then reroute the capture stream to "Monitor of ..."
  by hand in pavucontrol.
- macOS: BlackHole plus a multi-output device.

**Other pain points**

- LedFx's own troubleshooting page calls it "extremely latency sensitive" and points at WiFi.
  [Troubleshooting](https://ledfx.readthedocs.io/en/latest/trouble.html)
- No timeline, and limited manual performance control.

**Compared with darkRGB 0.11.0 (2026-11-17)**

The owner asked for the gap to be closed where it could be, with leave to adapt a feature or leave one
out as long as it was said which. This is that list. It was first drawn up for 0.10.0 on 2026-11-16 from
LedFx's documentation for virtuals and its documentation index; the effect settings and device list are
from an earlier reading of its source and were not re-checked.
[Virtuals](https://docs.ledfx.app/en/latest/howto/virtuals.html), [index](https://docs.ledfx.app/en/latest/)

| LedFx has | darkRGB 0.11.0 | Taken as it is, changed, or left out |
|---|---|---|
| A virtual made of segments: any pixel range of any device, each flippable and reorderable | A strip cut into zones, by LEDs or by percent, each a light of its own; any lights grouped | Changed. A zone is a run of one device. Pieces are not reordered inside one light: place and group the zones on the stage instead. |
| Mapping: span or copy | Across the stage, on each light, or end to end | Taken, and one added: by position on the stage. |
| Grouping: several pixels act as one | Blocks | Changed: a setting of an effect, not of a light, since several effects can share a light here. |
| Centre offset | Slide along | Changed in the same way. |
| Frequency min and max for a virtual | A band or any range, for each effect or a whole folder | Changed in the same way. |
| Rows, to make a virtual a matrix | A strip said to be snaked into a matrix, with its wiring; or the wiring left to WLED | Taken. |
| Transition time and eight modes | A fade time and seven kinds: fade, brighten (LedFx's "add"), through black, through white, wipe, iris, dissolve | Changed. Push and slide move one picture off as the other comes on; here a wipe crosses the stage instead, because lights are placed and have no shared picture to slide. "None" is a fade time of nought. One setting for the show, not one per light. |
| Per-effect blur, flip, mirror, background colour | Blur, mirror, repeat, kaleidoscope, angle, spin, colour shift | Blur taken. Background colour left out: a Solid colour effect underneath does it, and can be faded and tied to the music like any other. |
| About 70 effects | 39 | Nine added: Take turns, Stage beams, Note colours (their pitch effects), Waterfall, Glitch, Lava lamp, Code rain (digital rain), Life (game of life), Moving picture (GIF player). Still fewer. Many of the rest are variations that settings here already give (a second kind of bars, scroll, strobe); they were not copied one for one. |
| GIF player | Moving picture: GIFs, and video too | Taken and widened. |
| Pitch detection, with methods to choose | The note that stands out, as a place in the octave, for effects and for any slider to follow | Changed. One method, no choice of method: which octave is not reported, only which note. |
| Melbank and onset settings | Fixed analysis | Left out. The analysis here sets itself; settings for it would be a way to get it wrong. Sensitivity and each effect's band remain. |
| Devices: WLED, E1.31, Art-Net, DDP, OpenRGB, Nanoleaf, LIFX, Hue, Govee, serial, Launchpad and more | WLED (DDP), sACN (E1.31), Art-Net, OpenRGB | OpenRGB taken, from its published protocol, tested against a stand-in and not yet against OpenRGB itself. Left out for now: Nanoleaf, LIFX, Hue and Govee (each needs its maker's pairing and a device to check against; most light as one colour or a few panels, not as pixels), serial strips (needs a strip on a serial adapter to check), Launchpad as a display. |
| Integrations: Spotify, QLC+, Home Assistant, MQTT; a REST API | A WebSocket API and a phone remote | Left out. Home Assistant and MQTT are put off at the owner's word. Spotify needs a paid account and keys; the timeline against a song file does that job here. QLC+ is for people with DMX fixtures. |
| Playlists of scenes | Autopilot, sets of scenes, Next by kind or by the song, and a timeline | Changed. |
| Runs on a Raspberry Pi and macOS | Windows and Linux, x86-64 | Left out at the owner's word ("windows and linux is fine for now"). |
| Audio from another device over the network (Sendspin) | The computer's own sound, or any input | Left out. |
| Copying the screen's colours | None | Left out for now: it needs a screen-capture path for each system. |

Where darkRGB is ahead: no audio setup at all; a stage where lights are placed and patterns cross them by
position; bent strips; a beat that is kept through breakdowns, with bars, nudge and tap; live pads, keys
and MIDI; a timeline; undo; a flash limit; a power limit by current.

## Other tools, one idea each (unverified)

- **Jinx!, Glediator, MADRIX:** two effect decks with a crossfader for live mixing on matrices.
- **Resolume, MadMapper:** clip-launch grid and pixel mapping.
- **SoundSwitch:** songs are analysed ahead of time and lighting follows the beat grid automatically.
- **MaestroDMX:** an "autopilot" that makes a decent show from audio with a few mood sliders and no programming.
- **Hyperion, HyperHDR:** priority-based arbitration when several sources want the same lights.
- **QLC+:** virtual console of user-arranged buttons and sliders.
- **Chataigne:** bridges protocols (OSC, MIDI, Art-Net) with a state machine and timeline.
- **WLED audio sync:** WLED devices share audio features over UDP multicast `239.0.0.1:11988`, so an app could
  drive WLED's on-device audio effects without streaming pixels.
  [Spec](https://mm.kno.wled.ge/WLEDSR/UDP-Sound-Sync/) (the port and address are verified; using it as a sender
  is an idea, not tested)

## Features worth copying

- Layout of models and groups, with effects that span several devices (xLights models, LedFx virtuals).
- Effect layers with blend modes and transitions; curves on any parameter.
- Automatic beat and bar grid for a song, plus tap-in marks.
- WLED discovery and DDP by default; no universe arithmetic for the user.
- Scenes that save and recall everything.
- `.fseq` v2 export and FPP upload, and xLights layout import, so existing yard displays can adopt the app.
- Clip grid and crossfader for live use.

## Pain points to avoid

- Any manual audio routing.
- Renders that block editing. Rendering should be real-time or in the background.
- Channel and universe numbers in the user's face.
- A dense first screen. A new user should have lights reacting to music within a couple of minutes.
- Real-time and sequenced modes living in separate programs.
