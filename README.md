# darkRGB

A modern, visual LED show controller for Windows and Linux. One app for three jobs that existing tools split
between them:

- **Audio-reactive:** lights follow whatever the computer is playing, with no audio routing setup.
- **Live:** trigger effects, colours and scenes from the keyboard or a MIDI controller, like a lighting console.
- **Timeline:** pre-programmed shows synced to a song.

It targets ESP32 controllers running [WLED](https://kno.wled.ge/) first, then generic sACN and Art-Net pixel
controllers.

![darkRGB with demo lights reacting to music](docs/images/main.png)

## Status

Version 0.2.0, in development. The first mode, audio-reactive, works end to end. Live and timeline modes are not
started.

| | Linux | Windows |
|---|---|---|
| Automated tests | pass | pass |
| System audio with no setup | works (PulseAudio and PipeWire, tested with virtual speakers) | works (checked on one Windows PC) |
| The app window | works (tested under a virtual display) | builds; not yet run by anyone |
| Real WLED hardware | not yet tried | not yet tried |

The effects have only met synthetic test songs so far, so expect them to need tuning against real music. See
[docs/roadmap.md](docs/roadmap.md) for the details.

What works today:

- Opens as one window. Finds WLED devices on the network, adds them by address, or scans the network for them.
- Demo lights and virtual lights that exist only on screen, for trying effects and planning layouts without
  hardware.
- Captures system audio with no setup and analyses it: loudness, 32 frequency bands, hits in bass, mids and
  highs, tempo and beats.
- Ten effects in layers. Each can play on all lights or chosen ones, and react to everything, bass, mids,
  highs or a frequency range you pick.
- A live preview where strips and matrices are arranged by dragging.
- Scenes, master brightness and blackout. The show saves itself.
- Simple by default; an Advanced switch reveals the rest.

## Try it

Builds for Windows and Linux are attached to runs of the [CI workflow](../../actions/workflows/ci.yml): open
the latest run, download the zip for your system, unzip. The zip is named with the version and build number,
which the app also shows in its bottom corner. Builds are started by hand ("Run workflow" on that page), not on
every push, and downloads are kept for a week.

| Program | What it is |
|---|---|
| `darkrgb` | The app. Double-click it. On first run, choose "Try it with demo lights" if you have no hardware to hand. |
| `darkrgb-cli` | The same engine from a terminal. Opens the UI in your browser, and has the checks below. |

Windows will warn that the programs are unsigned. The app window uses the system's web engine: WebView2 on
Windows (part of Windows 11, and installed with Edge on Windows 10) and WebKitGTK 4.1 on Linux
(`libwebkit2gtk-4.1-0`, present on most desktops). `darkrgb-cli` needs neither.

```
darkrgb-cli                start the engine and open the UI in a browser
darkrgb-cli --lan          also let phones on your network open it (no password yet)
darkrgb-cli audio-test     check that system audio is captured with no setup
darkrgb-cli ddp-test IP    stream a test pattern to one WLED device and report how steady it was
darkrgb-cli discover       list WLED devices found on the network
darkrgb-cli --help         all options
```

### The check that still needs real hardware

Run `darkrgb-cli ddp-test` followed by a WLED device's IP address. A white dot should glide along the lights
without stutter for 20 seconds, and the command prints how steady the stream was. Please report what it prints,
the WLED version, the LED count, and whether the device is on WiFi or Ethernet.

## Build from source

Needs [Rust](https://rustup.rs/) (the version in `rust-toolchain.toml` is installed for you) and
[Node.js](https://nodejs.org/) 22 with [pnpm](https://pnpm.io/). The app window also needs, on Linux:

```
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev librsvg2-dev libayatana-appindicator3-dev
```

```
cd ui && pnpm install && pnpm build && cd ..
cargo build --release -p darkrgb-desktop -p darkrgb-engine
./target/release/darkrgb
```

The UI is embedded in the programs, so build it first. `cargo test` runs the tests, including end-to-end tests
that drive a simulated device; it does not build the app window, so it needs none of the Linux packages above.

To work on the UI with live reload, run the engine and Vite side by side:

```
cargo run -p darkrgb-engine -- --no-open
cd ui && pnpm dev          # then open http://localhost:5173
```

`cargo run -p darkrgb-sim` starts pretend WLED devices that speak the real network protocol, for testing
discovery and streaming without hardware.

## Layout

```
crates/core      layout, layers, effects, renderer (no I/O)
crates/audio     audio capture and analysis
crates/output    DDP, WLED info, discovery, network scan
crates/engine    the engine as a library, and darkrgb-cli
crates/desktop   darkrgb, the app window (Tauri)
crates/sim       a pretend WLED device for tests
ui/              the interface (React, TypeScript, Vite)
docs/            research, architecture, roadmap
```

- [docs/research.md](docs/research.md): what xLights, Vixen and LedFx do well, where they hurt, and the gap.
- [docs/architecture.md](docs/architecture.md): the stack and the core design.
- [docs/roadmap.md](docs/roadmap.md): what is proven, what comes next, open decisions.
- [CHANGELOG.md](CHANGELOG.md): what's new in each version.

## Licence

GPL-3.0-or-later. See [LICENSE](LICENSE).
