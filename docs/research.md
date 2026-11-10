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
