# OpenLightShow

A modern, visual LED show controller for Windows and Linux. One app for three jobs that existing tools split
between them:

- **Audio-reactive:** lights follow whatever the computer is playing, with no audio routing setup.
- **Live:** trigger effects, colours and scenes from the keyboard or a MIDI controller, like a lighting console.
- **Timeline:** pre-programmed shows synced to a song.

It targets ESP32 controllers running [WLED](https://kno.wled.ge/) first, then generic sACN and Art-Net pixel
controllers.

## Status

Planning. There is no code yet. The docs below are the result of a survey of xLights, Vixen, LedFx and others
done on 2026-11-10.

- [docs/research.md](docs/research.md): what the existing tools do well, where they hurt, and the gap.
- [docs/architecture.md](docs/architecture.md): the chosen stack (Rust engine, React UI, Tauri shell) and the
  core design.
- [docs/roadmap.md](docs/roadmap.md): risk spikes, milestones and open decisions.