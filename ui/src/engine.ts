// Connection to the engine, and the types of everything it sends.
//
// Slow-changing data (the show, status) goes through React via
// useSyncExternalStore. Fast data (preview colours 30 times a second,
// audio features) is kept out of React and read by canvases directly.

import { useSyncExternalStore } from "react";

export type ParamValue = boolean | number | string;

export interface ParamSpec {
  id: string;
  label: string;
  /** Hidden until the user switches advanced settings on. */
  advanced: boolean;
  kind: "float" | "color" | "bool" | "choice" | "palette" | "words" | "picture";
  min?: number;
  max?: number;
  step?: number;
  options?: string[];
  default: ParamValue;
}

export interface EffectInfo {
  id: string;
  name: string;
  description: string;
  /** The group it is listed under in the picker. */
  category: string;
  audio: boolean;
  /**
   * What it has to do with fast flashing. "capped": it flashes on purpose
   * and slows itself to the show's flash limit. "needed": it only makes
   * sense flashing fast, and is off while the limit is on.
   */
  flash: "none" | "capped" | "needed";
  params: ParamSpec[];
}

export interface Palette {
  id: string;
  name: string;
  stops: string[];
  cyclic: boolean;
}

/** Who a show is for. */
export type Kind = "room" | "display" | "club" | "custom";

/** Something a setting can follow. Each runs from 0 to 1. */
export type Source =
  | { type: "level" | "bass" | "mid" | "treble" | "beat" | "hit" | "energy" }
  | { type: "wave" | "ramp" | "square" | "random"; beats: number }
  | { type: "fader"; n: number };

/** One setting of an effect that moves by itself. */
export interface Link {
  /** The setting's id, or "opacity" for the effect's strength. */
  param: string;
  source: Source;
  /** -1 to 1: how far it moves, as a share of the slider's range. */
  amount: number;
  /** 0 to 1: how much the source is smoothed first. Left out when not at all. */
  smooth?: number;
}

/** A pattern folded into mirror images of itself. */
export type Mirror = "off" | "across" | "down" | "both";

/** A kind of light an effect can be kept to. */
export type Only = "any" | "strips" | "matrices";

export interface Hello {
  app: string;
  version: string;
  /** CI build number, when this program was built there. */
  build: string | null;
  /** Edges of the 32 frequency bands in Hz (33 numbers). */
  band_hz: number[];
  /** First band and one past the last band of bass, mids and treble. */
  band_groups: [number, number][];
  /** Frame rates the engine can run at. */
  frame_rates: number[];
  /** Every starter scene there is. */
  looks: { name: string; description: string }[];
  /** The kinds of show, and the starter scenes each gets. */
  kinds: { id: Kind; name: string; description: string; looks: string[] }[];
  effects: EffectInfo[];
  palettes: Palette[];
}

export interface Device {
  id: number;
  name: string;
  /** A virtual light exists only in the preview. sACN and Art-Net are
   *  pixel controllers that are sent universes of 170 LEDs. */
  kind: "wled" | "virtual" | "sacn" | "artnet";
  /** One of the built-in demo lights. */
  demo: boolean;
  address: string;
  port: number;
  pixel_count: number;
  enabled: boolean;
  gamma: number;
  brightness: number;
  mac: string | null;
  version: string | null;
  /** For sACN and Art-Net: the universe of the first LED. */
  universe?: number;
  /** For sACN and Art-Net: the order a pixel's colours are sent in. */
  order?: ColourOrder;
  /** The most current the lights may draw, in milliamps. 0 or absent: no limit. */
  max_milliamps?: number;
  /** Frames are held back this many milliseconds before they are sent. */
  delay_ms?: number;
  /** Dim levels between two steps are made up over time. */
  dither?: boolean;
}

export type ColourOrder = "rgb" | "grb" | "brg" | "rbg" | "gbr" | "bgr";

/** LEDs that fit in one universe of sACN or Art-Net. */
export const PIXELS_PER_UNIVERSE = 170;

/** The universes a pixel controller's LEDs use, first and last. */
export function universesOf(first: number, pixels: number): [number, number] {
  return [first, first + Math.max(1, Math.ceil(pixels / PIXELS_PER_UNIVERSE)) - 1];
}

export type Shape =
  | { type: "strip"; count: number; reverse: boolean }
  | { type: "matrix"; cols: number; rows: number; serpentine: boolean };

export interface Fixture {
  id: number;
  name: string;
  device: number;
  start: number;
  shape: Shape;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  /** The group of lights this fixture is filed under, if any. */
  group: number | null;
  /** A bent strip's corners, each from (0, 0) to (1, 1) in its box. Left out when straight. */
  path?: [number, number][];
  /** The path runs on from its last corner to its first. */
  closed?: boolean;
}

/** A named set of lights that can be aimed at, dimmed or switched as one. */
export interface LightGroup {
  id: number;
  name: string;
  enabled: boolean;
  brightness: number;
}

/** A folder of effects. */
export interface LayerGroup {
  id: number;
  name: string;
  enabled: boolean;
  opacity: number;
  /** Stays on top, and is left alone when the scene changes. */
  pinned: boolean;
}

/** What a key does. */
export type Action =
  | { type: "scene"; id: number }
  | { type: "layer"; id: number }
  | { type: "layer_group"; id: number }
  | { type: "light_group"; id: number }
  | { type: "blackout" }
  | { type: "tap_tempo" }
  | { type: "hit" }
  | { type: "next_scene" }
  | { type: "prev_scene" }
  | { type: "freeze" }
  | { type: "downbeat" }
  | { type: "colours"; id: number };

/**
 * What a press does: act on each press, act only while held down, or set
 * off the effect's reaction as if a beat had hit.
 */
export type KeyMode = "toggle" | "hold" | "fire";

/** A pad in the live view. Usually it has a key; it does not need one. */
export interface Binding {
  id: number;
  /** The key's place on the keyboard, such as `KeyA`, if the pad has one. */
  key: string | null;
  /** What is printed on the key. */
  label: string;
  action: Action;
  mode: KeyMode;
  /** The pad or key on a MIDI controller that plays this pad. */
  midi?: MidiNote;
  /** The page of pads it is on, 1 to 4. Left out when it is on every page. */
  page?: number;
}

/** Whether a pad is live on the page that is showing. */
export function onPage(binding: Binding, page: number): boolean {
  return !binding.page || binding.page === page;
}

/** A pad or key on a MIDI controller: channel 0 to 15, note 0 to 127. */
export interface MidiNote {
  channel: number;
  note: number;
}

/** What a knob or fader on a MIDI controller can move. */
export type KnobTarget = { type: "brightness" | "speed" | "sensitivity" } | { type: "fader"; n: number };

export type LearnTarget = { type: "pad"; binding: number } | { type: "knob"; target: KnobTarget };

export type Launch = "now" | "beat" | "bar";

export type Blend = "normal" | "add" | "lighten" | "multiply";

/** The part of the spectrum a layer listens to. */
export type Band =
  | { type: "all" }
  | { type: "bass" }
  | { type: "mid" }
  | { type: "treble" }
  | { type: "range"; low: number; high: number };

export interface Sound {
  band: Band;
  /** Multiplier on what the layer hears, 0.25 to 4. */
  gain: number;
  /** How much the layer's brightness follows the sound, 0 to 1. */
  pulse: number;
  /** The layer ignores the music and reacts only when fired by hand. */
  manual: boolean;
}

export interface Layer {
  id: number;
  /** A label the user gave it; otherwise the effect's name is shown. */
  name: string | null;
  sound: Sound;
  effect: string;
  params: Record<string, ParamValue>;
  opacity: number;
  blend: Blend;
  /** Ids of fixtures and groups of lights; empty means every light. */
  targets: number[];
  enabled: boolean;
  /** The folder this layer is filed in, if any. */
  group: number | null;
  /** Drawn on each light by itself instead of once across all of them. */
  each: boolean;
  /** Kept to strips or to matrices, among whatever it plays on. */
  only: Only;
  /** Settings that follow something. Left out when there are none. */
  links?: Link[];
  /** Degrees the pattern is turned, clockwise. Left out when 0. */
  angle?: number;
  /** Degrees a second it keeps turning. Left out when 0. */
  spin?: number;
  /** Drawn on each light, the pattern turns with a turned matrix instead of staying upright. */
  turns?: boolean;
  /** Folded into mirror images. Left out when not. */
  mirror?: Mirror;
  /** Copies side by side, 1 to 8. Left out when 1. */
  repeat?: number;
  /** Wedges of a kaleidoscope, 2 to 12. Left out when off. */
  kaleido?: number;
  /** How far the pattern shifts from one light to the next, 0 to 1. */
  spread?: number;
  /** Degrees every colour is shifted round the colour wheel. */
  hue?: number;
}

export interface Scene {
  id: number;
  name: string;
  layers: Layer[];
  groups: LayerGroup[];
  /** Whether autopilot may switch to this scene. */
  auto: boolean;
  /** The parts of a song autopilot uses it for. */
  mood: Mood;
}

/** The parts of a song a scene suits. */
export type Mood = "any" | "calm" | "big";

export interface Show {
  version: number;
  name: string;
  /** Who the show is for; it decided what the show began with. */
  kind: Kind;
  devices: Device[];
  fixtures: Fixture[];
  light_groups: LightGroup[];
  layers: Layer[];
  layer_groups: LayerGroup[];
  scenes: Scene[];
  bindings: Binding[];
  master: { brightness: number; blackout: boolean; limit: number };
  audio: {
    source: string;
    sensitivity: number;
    delay_ms: number;
    /** How much the lights follow the shape of the song, 0 to 1. */
    dynamics: number;
    /** Milliseconds the lights run ahead of the beat they hear. */
    lead_ms?: number;
  };
  live: {
    launch: Launch;
    fade: number;
    speed: number;
    /** Autopilot: changes scene by itself, in time with the music. */
    auto: { enabled: boolean; every_bars: number; shuffle: boolean; on_drops: boolean };
    /** Where the four faders of the Live view stand, 0 to 1. */
    faders: number[];
    /** The scene to change to when the music stops, and after how many seconds. */
    idle: { scene: number | null; after: number };
    /** Knobs and faders of a MIDI controller, and what each moves. Left out when there are none. */
    knobs?: { channel: number; cc: number; target: KnobTarget }[];
    /** The page of pads that is showing, 1 to 4. */
    page: number;
    /** Colours that a pad can give to everything that is playing. */
    swatches?: { id: number; name: string; value: string }[];
    /** The swatch in force, if any. */
    colours?: number;
    /** Take the beat from a MIDI clock when one is arriving. */
    midi_clock?: boolean;
  };
  output: {
    frame_rate: number;
  };
  /** The hours the lights keep each day, in minutes after midnight. */
  schedule: { enabled: boolean; on: number; off: number };
  /** How this show treats fast flashing. */
  flash: {
    /** Most flashes of the whole picture in a second; 0 means no limit. */
    limit: number;
    /** Count small changes as flashes too, as guidance for screens does. */
    strict: boolean;
    /** False until whoever makes the show has chosen. */
    decided: boolean;
  };
  /** The show laid out in time. Left out until someone begins one. */
  timeline?: Timeline;
}

/** What is happening live, beyond what is saved in the show. */
export interface Live {
  /** Ids of the keys being held down. */
  held: number[];
  /** The scene waiting for the beat. */
  pending_scene: number | null;
  /** The tempo tapped by hand, or 0 when the music's own is followed. */
  tapped_bpm: number;
  /** The picture has been frozen by a press. */
  frozen: boolean;
  /** The music has stopped and the scene chosen for that is showing. */
  idle: boolean;
  /** The show's schedule has the lights off at this time of day. */
  asleep: boolean;
  /** Where the timeline is, as of the engine's last frame. */
  timeline?: { playing: boolean; recording: boolean; position: number };
  /** Where the beat comes from, and how it has been corrected by hand. */
  clock?: { source: "music" | "tapped" | "midi"; scale: number; nudge: number; midi_bpm: number | null };
}

/** What a clip on the timeline plays: a scene from then on, or a pad held for its length. */
export type ClipKind = { type: "scene"; id: number } | { type: "pad"; id: number };

export interface Clip {
  id: number;
  /** Seconds from the start. */
  start: number;
  /** Seconds it lasts. 0 for a scene clip, which lasts until the next. */
  length: number;
  what: ClipKind;
}

/** A show laid out in time, usually against a song. */
export interface Timeline {
  song: { file: string; name: string } | null;
  /** Seconds from start to end. */
  length: number;
  /** The tempo of the grid clips snap to; 0 for no grid. */
  bpm: number;
  /** Where the grid's first beat falls, in seconds. */
  offset: number;
  repeat: boolean;
  clips: Clip[];
}

/** What a show has before anyone touches its timeline. */
export const NO_TIMELINE: Timeline = { song: null, length: 180, bpm: 0, offset: 0, repeat: false, clips: [] };

export interface DeviceStats {
  frames_sent: number;
  frames_dropped: number;
  resolved: boolean;
  /** What the last frame would draw, by estimate, in milliamps. */
  milliamps?: number;
  /** The last frame was dimmed to keep to the current limit. */
  limited?: boolean;
}

export interface Discovered {
  name: string;
  address: string;
  leds: number;
  matrix: [number, number] | null;
  version: string;
  mac: string | null;
  added: boolean;
}

export interface Status {
  render: {
    fps: number;
    frame_ms: number;
    late_frames: number;
    /** The flash limit held the picture back in the last second. */
    flash_limited: boolean;
    /** Times the frame loop has failed and been started again. */
    restarts?: number;
    devices: Record<string, DeviceStats>;
  };
  audio: {
    state: "starting" | "running" | "error";
    source: string;
    device: string | null;
    sample_rate: number;
    channels: number;
    message: string | null;
  };
  inputs: { id: string; name: string }[];
  /** Local networks this computer is on, best guess first. */
  networks: { address: string; prefix: string }[];
  scan: { running: boolean; prefix: string; done: number; total: number; found: number };
  discovered: Discovered[];
  notice: string | null;
  /** Where the show is saved on the computer running the engine. */
  show_file: string | null;
  /** Whether other devices on the network may connect, as a remote. */
  remote?: {
    /** The engine is listening on the network now. */
    on: boolean;
    /** What was chosen in Settings, for the next start. */
    wanted: boolean;
    port: number;
    /** This page is open on the engine's own computer. */
    local: boolean;
    /** The code other devices are asked for. Only the computer itself is told. */
    code: string | null;
    /** False when the choice cannot be kept (started with one show file). */
    available: boolean;
  };
  /** MIDI controllers: whether there is MIDI at all, which are heard, the last thing heard. */
  midi?: { available: boolean; ports: string[]; learning: LearnTarget | null; last: string | null };
}

/** One show in the library on the engine's computer. */
export interface ShowEntry {
  /** The file's name in the shows folder; how the show is asked for. */
  file: string;
  name: string;
  /** When it was last saved, in seconds since 1970. */
  modified: number;
  lights: number;
  effects: number;
  scenes: number;
  /** False for a file that could not be understood. */
  readable: boolean;
}

/** An effect with its settings, or a whole look, kept to use in any show. */
export interface SavedPattern {
  id: number;
  name: string;
  /** A look becomes a scene when used; an effect is added to what plays. */
  look: boolean;
  layers: { effect: string; name: string | null; params: Record<string, ParamValue> }[];
}

/** A palette of the user's own, kept under a name. */
export interface SavedPalette {
  id: number;
  name: string;
  /** The colours, as a palette setting holds them. */
  value: string;
}

/**
 * A palette setting's value as a palette: one of the built-in ones by id,
 * or the user's own colours written out, such as "#ff0000,#ffffff|loop".
 */
export function readPalette(value: string, builtIn: Palette[]): Palette | undefined {
  if (!value.startsWith("#")) return builtIn.find((p) => p.id === value);
  const cyclic = value.endsWith("|loop");
  const stops = (cyclic ? value.slice(0, -5) : value).split(",");
  if (stops.length < 2 || stops.some((s) => !/^#[0-9a-fA-F]{6}$/.test(s))) return undefined;
  return { id: value, name: "Your own", stops, cyclic };
}

/** The written form of a palette of the user's own. */
export function writePalette(stops: string[], cyclic: boolean): string {
  return stops.join(",") + (cyclic ? "|loop" : "");
}

export interface Library {
  /** False when the engine was started with one show file of its own. */
  available: boolean;
  /** The file of the show that is open. */
  current: string | null;
  folder: string | null;
  shows: ShowEntry[];
  patterns: SavedPattern[];
  palettes: SavedPalette[];
}

export interface AudioFeatures {
  level: number;
  bass: number;
  mid: number;
  treble: number;
  bands: number[];
  onset: number;
  onsets: number[];
  beat: number;
  beat_phase: number;
  bpm: number;
  beat_count: number;
  active: boolean;
  /** How big this part of the song is against the last minute, 0 to 1. */
  intensity: number;
  /** Goes up by one when the song moves to a new section. */
  section: number;
  /** Jumps to 1 on a drop, then fades. */
  drop: number;
  /** Which beat of the bar this is, 0 for the first. */
  bar_beat?: number;
  /** The beat is being kept from memory: the music is not giving one just now. */
  coasting?: boolean;
}

export type Command =
  | { type: "add_device"; address: string }
  | { type: "scan_network"; prefix: string }
  | { type: "set_demo"; enabled: boolean }
  | { type: "timeline_play"; from?: number }
  | { type: "timeline_stop" }
  | { type: "timeline_seek"; position: number }
  | { type: "timeline_sync"; position: number }
  | { type: "timeline_record"; on: boolean }
  | { type: "set_timeline"; length?: number; bpm?: number; offset?: number; repeat?: boolean }
  | { type: "add_clip"; start: number; length?: number; what: ClipKind }
  | { type: "update_clips"; clips: { id: number; start?: number; length?: number }[] }
  | { type: "remove_clips"; ids: number[] }
  | { type: "clear_timeline" }
  | { type: "remove_song" }
  | { type: "set_remote"; enabled: boolean }
  | { type: "add_looks" }
  | { type: "set_scene_auto"; id: number; auto: boolean }
  | {
      type: "add_virtual";
      name: string;
      shape: { type: "strip"; count: number } | { type: "matrix"; cols: number; rows: number };
    }
  | { type: "rename_layer"; id: number; name: string }
  | { type: "duplicate_layer"; id: number }
  | { type: "set_sound"; layer: number; band?: Band; gain?: number; pulse?: number; manual?: boolean }
  | { type: "remove_device"; id: number }
  | {
      type: "connect";
      device: number;
      to:
        | { type: "wled"; address: string }
        | { type: "pixels"; protocol: "sacn" | "artnet"; address: string; port?: number; universe: number }
        | { type: "nothing" };
    }
  | {
      type: "update_device";
      id: number;
      name?: string;
      enabled?: boolean;
      gamma?: number;
      brightness?: number;
      address?: string;
      universe?: number;
      order?: ColourOrder;
      max_milliamps?: number;
      delay_ms?: number;
      dither?: boolean;
    }
  | {
      type: "add_pixels";
      name: string;
      protocol: "sacn" | "artnet";
      address: string;
      universe: number;
      shape: { type: "strip"; count: number } | { type: "matrix"; cols: number; rows: number };
    }
  | {
      type: "update_fixture";
      id: number;
      name?: string;
      x?: number;
      y?: number;
      width?: number;
      height?: number;
      rotation?: number;
      reverse?: boolean;
    }
  | { type: "add_layer"; effect: string; group?: number | null }
  | { type: "remove_layer"; id: number }
  | { type: "arrange_layers"; order: { id: number; group: number | null }[] }
  | { type: "add_layer_group"; name: string; layer?: number }
  | { type: "update_layer_group"; id: number; name?: string; enabled?: boolean; opacity?: number; pinned?: boolean }
  | { type: "remove_layer_group"; id: number; keep_layers: boolean }
  | { type: "add_light_group"; name: string; fixtures?: number[] }
  | { type: "update_light_group"; id: number; name?: string; enabled?: boolean; brightness?: number }
  | { type: "remove_light_group"; id: number }
  | { type: "group_lights"; fixtures: number[]; group: number | null }
  | { type: "bind"; key: string | null; label: string; action: Action; mode: KeyMode }
  | { type: "update_binding"; id: number; mode: KeyMode }
  | { type: "clear_key"; id: number }
  | { type: "unbind"; id: number }
  | { type: "arrange_pads"; order: number[] }
  | { type: "add_quick_pad"; kind: "flash" | "strobe" | "pulse" | "lightning" }
  | { type: "trigger"; binding: number; down: boolean }
  | { type: "perform"; action: Action }
  | { type: "tap" }
  | { type: "hit" }
  | { type: "auto_tempo" }
  | { type: "tempo_scale"; scale: number }
  | { type: "nudge"; beats: number }
  | { type: "downbeat" }
  | {
      type: "set_live";
      launch?: Launch;
      fade?: number;
      speed?: number;
      auto?: boolean;
      every_bars?: number;
      shuffle?: boolean;
      on_drops?: boolean;
  midi_clock?: boolean;
    }
  | { type: "set_output"; frame_rate?: number }
  | { type: "set_flash"; limit?: number; strict?: boolean; decided?: boolean }
  | { type: "rename_scene"; id: number; name: string }
  | { type: "new_show"; name?: string; keep_lights: boolean; kind?: Kind }
  | { type: "begin"; kind: Kind; demo: boolean }
  | { type: "open_show"; file: string }
  | { type: "duplicate_show"; file: string }
  | { type: "delete_show"; file: string }
  | { type: "rename_show_file"; file: string; name: string }
  | { type: "refresh_library" }
  | { type: "save_pattern"; layer: number; name?: string }
  | { type: "save_look"; name: string }
  | { type: "use_pattern"; id: number; group?: number | null }
  | { type: "rename_pattern"; id: number; name: string }
  | { type: "delete_pattern"; id: number }
  | { type: "save_palette"; name: string; value: string }
  | { type: "delete_palette"; id: number }
  | { type: "export_show" }
  | {
      type: "update_layer";
      id: number;
      effect?: string;
      opacity?: number;
      blend?: Blend;
      enabled?: boolean;
      targets?: number[];
      each?: boolean;
      only?: Only;
      angle?: number;
      spin?: number;
      turns?: boolean;
      mirror?: Mirror;
      repeat?: number;
      kaleido?: number;
      spread?: number;
      hue?: number;
    }
  | { type: "set_path"; fixture: number; points: [number, number][]; closed: boolean }
  | { type: "set_param"; layer: number; param: string; value: ParamValue }
  | { type: "reset_params"; layer: number }
  | { type: "set_link"; layer: number; param: string; source: Source; amount: number; smooth?: number }
  | { type: "remove_link"; layer: number; param: string }
  | { type: "set_fader"; n: number; value: number }
  | { type: "add_colour_pad"; name: string; value: string }
  | { type: "set_pad_page"; id: number; page: number }
  | { type: "set_page"; page: number }
  | { type: "set_schedule"; enabled?: boolean; on?: number; off?: number }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "midi_learn"; target: LearnTarget | null }
  | { type: "set_pad_midi"; id: number; midi: MidiNote | null }
  | { type: "set_knob"; target: KnobTarget; channel: number; cc: number }
  | { type: "remove_knob"; target: KnobTarget }
  | { type: "set_scene_mood"; id: number; mood: Mood }
  | { type: "set_idle"; scene: number | null; after?: number }
  | { type: "set_master"; brightness?: number; blackout?: boolean; limit?: number }
  | { type: "set_audio"; source?: string; sensitivity?: number; delay_ms?: number; dynamics?: number; lead_ms?: number }
  | { type: "save_scene"; name: string }
  | { type: "recall_scene"; id: number }
  | { type: "delete_scene"; id: number }
  | { type: "rename_show"; name: string }
  | { type: "dismiss_notice" };

export interface CommandResult {
  ok: boolean;
  note?: string | null;
  error?: string;
}

export interface Toast {
  id: number;
  kind: "error" | "info";
  text: string;
}

const MSG_PREVIEW = 1;
const MSG_LAYOUT = 2;

const SILENCE: AudioFeatures = {
  level: 0,
  bass: 0,
  mid: 0,
  treble: 0,
  bands: new Array<number>(32).fill(0),
  onset: 0,
  onsets: [0, 0, 0],
  beat: 0,
  beat_phase: 0,
  bpm: 0,
  beat_count: 0,
  active: false,
  intensity: 1,
  section: 0,
  drop: 0,
};

export class Store<T> {
  private listeners = new Set<() => void>();
  constructor(private value: T) {}
  get = (): T => this.value;
  set(value: T): void {
    this.value = value;
    for (const listener of this.listeners) listener();
  }
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
}

class Engine {
  readonly connected = new Store(false);
  /** True while this device has to give the pairing code before it is let in. */
  readonly needsCode = new Store(false);
  readonly hello = new Store<Hello | null>(null);
  readonly show = new Store<Show | null>(null);
  readonly status = new Store<Status | null>(null);
  readonly live = new Store<Live>({ held: [], pending_scene: null, tapped_bpm: 0, frozen: false, idle: false, asleep: false });
  readonly library = new Store<Library>({
    available: false,
    current: null,
    folder: null,
    shows: [],
    patterns: [],
    palettes: [],
  });
  /** The scene that is playing exactly as it was saved, if any. */
  readonly currentScene = new Store<number | null>(null);
  /** Whether there is a change to take back, and one to put back. */
  readonly history = new Store({ undo: false, redo: false });
  readonly toasts = new Store<Toast[]>([]);

  /** Latest audio features. Read from animation frames, not from React. */
  audio: AudioFeatures = SILENCE;
  /** Canvas x,y of every pixel, in the order preview colours arrive. */
  positions = new Float32Array(0);
  /** RGB for every pixel; only valid when `colorsSerial === layoutSerial`. */
  colors = new Uint8Array(0);
  layoutSerial = 0;
  colorsSerial = -1;
  /** Bumped whenever positions change, so the stage can rebuild buffers. */
  layoutVersion = 0;

  /** When the last word on the live state arrived, to count on from. */
  private liveAt = 0;
  private socket: WebSocket | null = null;
  private nextId = 1;
  private pending = new Map<number, (result: CommandResult) => void>();
  private nextToast = 1;

  /**
   * Connects, after asking whether this device is let in. A device other
   * than the engine's own computer has to give a code first; until it
   * has, the engine refuses the connection, and a browser cannot tell a
   * refusal from an engine that is not running.
   */
  connect(): void {
    if (this.socket && this.socket.readyState !== WebSocket.CLOSED) return;
    void fetch("/api/access")
      .then((response) => (response.ok ? response.json() : null))
      .then((access: { paired?: boolean } | null) => access?.paired === false)
      // No answer at all: the engine is not running, or is older. Try anyway.
      .catch(() => false)
      .then((needsCode) => {
        this.needsCode.set(needsCode);
        if (needsCode) setTimeout(() => this.connect(), 3000);
        else this.open();
      });
  }

  /** Gives the pairing code typed on this device. */
  async pair(code: string): Promise<CommandResult> {
    try {
      const response = await fetch("/api/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code }),
      });
      const result = (await response.json()) as CommandResult;
      if (result.ok) {
        this.needsCode.set(false);
        this.open();
      }
      return result;
    } catch {
      return { ok: false, error: "Could not reach the computer running darkRGB." };
    }
  }

  private open(): void {
    // One connection only, however often this is called.
    if (this.socket && this.socket.readyState !== WebSocket.CLOSED) return;
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const socket = new WebSocket(`${scheme}://${location.host}/ws`);
    socket.binaryType = "arraybuffer";
    this.socket = socket;
    socket.onopen = () => this.connected.set(true);
    socket.onmessage = (event) => {
      if (typeof event.data === "string") this.onText(event.data);
      else this.onBinary(event.data as ArrayBuffer);
    };
    socket.onclose = () => {
      this.connected.set(false);
      this.audio = SILENCE;
      // Commands in flight will never be answered now.
      for (const resolve of this.pending.values()) resolve({ ok: false, error: "Lost the connection to the engine." });
      this.pending.clear();
      setTimeout(() => this.connect(), 1000);
    };
  }

  private onText(text: string): void {
    const message = JSON.parse(text) as { type: string } & Record<string, unknown>;
    switch (message.type) {
      case "audio":
        this.audio = message as unknown as AudioFeatures;
        break;
      case "state":
        this.currentScene.set((message.current_scene as number | null) ?? null);
        this.history.set({ undo: message.can_undo === true, redo: message.can_redo === true });
        this.show.set(message.show as Show);
        break;
      case "live":
        this.liveAt = performance.now();
        this.live.set(message as unknown as Live);
        break;
      case "status":
        this.status.set(message as unknown as Status);
        break;
      case "library": {
        const next = message as unknown as Library;
        // Another show has other lights: nothing picked carries over.
        if (next.current !== this.library.get().current) selection.set([]);
        this.library.set(next);
        break;
      }
      case "hello":
        this.hello.set(message as unknown as Hello);
        break;
      case "result": {
        const result = message as unknown as CommandResult & { req: number | null };
        const resolve = result.req == null ? undefined : this.pending.get(result.req);
        if (resolve && result.req != null) {
          this.pending.delete(result.req);
          resolve(result);
        } else if (!result.ok && result.error) {
          this.toast("error", result.error);
        }
        break;
      }
    }
  }

  private onBinary(buffer: ArrayBuffer): void {
    if (buffer.byteLength < 8) return;
    const header = new DataView(buffer);
    const kind = header.getUint8(0);
    const serial = header.getUint32(4, true);
    if (kind === MSG_LAYOUT) {
      this.positions = new Float32Array(buffer.slice(8));
      this.layoutSerial = serial;
      this.layoutVersion += 1;
    } else if (kind === MSG_PREVIEW) {
      this.colors = new Uint8Array(buffer, 8);
      this.colorsSerial = serial;
    }
  }

  /** Sends a command and resolves with the engine's answer. */
  request(command: Command): Promise<CommandResult> {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return Promise.resolve({ ok: false, error: "Not connected to the engine." });
    }
    // The request's number has a field of its own: a command's `id` is
    // the device or effect it is about.
    const req = this.nextId++;
    return new Promise((resolve) => {
      this.pending.set(req, resolve);
      socket.send(JSON.stringify({ ...command, req }));
    });
  }

  /**
   * Sends a command and shows any error or note as a toast. For controls
   * where there is nothing more useful to do with the answer.
   */
  send(command: Command): void {
    void this.request(command).then((result) => {
      if (!result.ok) this.toast("error", result.error ?? "That did not work.");
      else if (result.note) this.toast("info", result.note);
    });
  }

  /**
   * Sends a command without asking for an answer. For streams of updates
   * while dragging; the engine still reports a failure, shown as a toast.
   */
  quiet(command: Command): void {
    const socket = this.socket;
    if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(command));
  }

  /**
   * Where the timeline is right now, in seconds. The engine says where
   * it was at its last word; while it plays, this counts on from there.
   */
  playhead(): number {
    const t = this.live.get().timeline;
    if (!t) return 0;
    return t.playing ? t.position + (performance.now() - this.liveAt) / 1000 : t.position;
  }

  /** Hands the engine a song for the timeline to play against. */
  async uploadSong(file: File): Promise<CommandResult> {
    try {
      const response = await fetch(`/api/song?name=${encodeURIComponent(file.name)}`, { method: "POST", body: file });
      if (response.status === 413) return { ok: false, error: "That file is too large to be a song." };
      return (await response.json()) as CommandResult;
    } catch {
      return { ok: false, error: "Could not reach the engine." };
    }
  }

  /**
   * Opens a show from a file: added to the library, or in place of the
   * show when there is no library. A whole show does not fit in a
   * WebSocket message, so this goes over HTTP.
   */
  async importShow(json: string): Promise<CommandResult> {
    try {
      const response = await fetch("/api/import", { method: "POST", body: json });
      if (response.status === 413) return { ok: false, error: "That file is far too large to be a show." };
      return (await response.json()) as CommandResult;
    } catch {
      return { ok: false, error: "Could not reach the engine." };
    }
  }

  toast(kind: Toast["kind"], text: string): void {
    // The engine words its errors as part of a sentence.
    text = text.charAt(0).toUpperCase() + text.slice(1);
    const id = this.nextToast++;
    this.toasts.set([...this.toasts.get().slice(-3), { id, kind, text }]);
    // Long messages stay long enough to be read.
    const reading = Math.min(20000, 3500 + text.length * 55);
    setTimeout(() => this.dismissToast(id), kind === "error" ? Math.max(8000, reading) : reading);
  }

  dismissToast(id: number): void {
    this.toasts.set(this.toasts.get().filter((t) => t.id !== id));
  }
}

export const engine = new Engine();

/**
 * A setting of the interface itself, kept in the browser. It belongs to
 * whoever is looking (a phone used as a remote keeps its own), not to
 * the show.
 */
export class Pref<T> {
  private store: Store<T>;
  constructor(
    private key: string,
    fallback: T,
  ) {
    let value = fallback;
    try {
      const saved = localStorage.getItem(key);
      if (saved !== null) value = JSON.parse(saved) as T;
    } catch {
      // Storage can be unavailable or hold something unreadable.
    }
    this.store = new Store(value);
  }
  get = (): T => this.store.get();
  subscribe = (listener: () => void) => this.store.subscribe(listener);
  set(value: T): void {
    this.store.set(value);
    try {
      localStorage.setItem(this.key, JSON.stringify(value));
    } catch {
      // Not being able to remember it is not worth an error.
    }
  }
}

/**
 * Which layout is shown. The simple one, for a new user, has what most
 * shows need and assumes the rest; the full one has every control.
 */
export const fullPref = new Pref<boolean>("darkrgb.advanced", false);
export const useFull = () => useSyncExternalStore(fullPref.subscribe, fullPref.get);

/** Building the show, or performing it with the panels out of the way. */
export type View = "edit" | "live" | "timeline";
export const viewPref = new Pref<View>("darkrgb.view", "edit");
export const useView = () => useSyncExternalStore(viewPref.subscribe, viewPref.get);

/** Folders and groups the user has folded shut, by id. */
export const foldedPref = new Pref<number[]>("darkrgb.folded", []);
export const useFolded = () => useSyncExternalStore(foldedPref.subscribe, foldedPref.get);
export function toggleFolded(id: number): void {
  const folded = foldedPref.get();
  foldedPref.set(folded.includes(id) ? folded.filter((f) => f !== id) : [...folded, id]);
}

/**
 * The lights picked on the stage, as fixture ids. Shared, so the Lights
 * list and the effects can see and change what is picked.
 */
export const selection = new Store<number[]>([]);
export const useSelection = () => useSyncExternalStore(selection.subscribe, selection.get);

/** Picks exactly these lights, or adds them to (or takes them from) what is picked. */
export function select(ids: number[], add = false): void {
  if (!add) return selection.set(ids);
  const current = selection.get();
  const allIn = ids.every((id) => current.includes(id));
  selection.set(allIn ? current.filter((id) => !ids.includes(id)) : [...new Set([...current, ...ids])]);
}

/**
 * The kind picked on the first-run screen. It is applied when the first
 * lights arrive, so changing one's mind before then costs nothing.
 */
export const firstKind = new Store<Kind>("room");
export const useFirstKind = () => useSyncExternalStore(firstKind.subscribe, firstKind.get);

let lightsAskedAt = 0;
/**
 * Called by whatever adds lights on this device. Only the device that
 * added a fresh show's first lights then sets the show up for its kind, so
 * a phone that happens to have the same screen open does not do it too.
 */
export function addingLights(): void {
  lightsAskedAt = Date.now();
}
/** The kind to set a fresh show up as, if this device just added its lights. */
export function takeFirstKind(): Kind | null {
  const mine = Date.now() - lightsAskedAt < 20000;
  lightsAskedAt = 0;
  return mine ? firstKind.get() : null;
}

/** Whether the window that lists the shows is open. */
export const libraryOpen = new Store(false);
export const useLibraryOpen = () => useSyncExternalStore(libraryOpen.subscribe, libraryOpen.get);
/** Which list the window shows when it next opens. */
export const libraryTab = new Store<"shows" | "kept">("shows");
export const useLibrary = () => useSyncExternalStore(engine.library.subscribe, engine.library.get);

/** Which page of the settings window is open, if it is open at all. */
export type SettingsPage = "general" | "sound" | "lights" | "live" | "show" | "about";
export const settingsPage = new Store<SettingsPage | null>(null);
export const useSettingsPage = () => useSyncExternalStore(settingsPage.subscribe, settingsPage.get);

export const useLive = () => useSyncExternalStore(engine.live.subscribe, engine.live.get);
export const useCurrentScene = () => useSyncExternalStore(engine.currentScene.subscribe, engine.currentScene.get);

export function sameAction(a: Action, b: Action): boolean {
  return a.type === b.type && ("id" in a ? a.id : 0) === ("id" in b ? b.id : 0);
}

export const useHistory = () => useSyncExternalStore(engine.history.subscribe, engine.history.get);
export const useConnected = () => useSyncExternalStore(engine.connected.subscribe, engine.connected.get);
export const useNeedsCode = () => useSyncExternalStore(engine.needsCode.subscribe, engine.needsCode.get);
export const useHello = () => useSyncExternalStore(engine.hello.subscribe, engine.hello.get);
export const useShow = () => useSyncExternalStore(engine.show.subscribe, engine.show.get);
export const useStatus = () => useSyncExternalStore(engine.status.subscribe, engine.status.get);
export const useToasts = () => useSyncExternalStore(engine.toasts.subscribe, engine.toasts.get);

/** Coalesces rapid calls (a dragged slider) into one per animation frame. */
export function perFrame<T>(run: (value: T) => void): (value: T) => void {
  let latest: T | undefined;
  let scheduled = false;
  return (value: T) => {
    latest = value;
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      run(latest as T);
    });
  };
}
