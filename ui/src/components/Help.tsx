// The help window: a quick start, and how each part of darkRGB is used.
// Written as plain data so it can be searched.

import { CircleHelp, Search, X } from "lucide-react";
import { useEffect, useState, useSyncExternalStore } from "react";
import { Store } from "../engine";

/** The topic the help window is open on, or null when it is shut. */
export const helpTopic = new Store<string | null>(null);

type Block = string | { steps: string[] } | { list: string[] } | { head: string };

interface Topic {
  id: string;
  title: string;
  blocks: Block[];
}

const TOPICS: Topic[] = [
  {
    id: "start",
    title: "Quick start",
    blocks: [
      "Five minutes from nothing to lights that move with your music. You do not need any hardware to try it.",
      {
        steps: [
          "Start a show. On first run, choose what you are lighting: a room, a light show for a house or garden, or a live show. Each begins with scenes ready to play. \"Try it with demo lights\" gives you two strips and a matrix on screen.",
          "Play some music on this computer. darkRGB listens to whatever the computer is playing; there is nothing to plug in. The meter at the top moves when it hears sound, and shows the tempo once it has found one.",
          "Press a scene. Scenes are the buttons along the bottom, or the number keys. Each is a complete look.",
          "Add your lights. In the Lights panel on the left, WLED devices on your network are listed by themselves: press Add. Or type an address. No hardware yet? Add virtual lights, lay them out, and connect them to real hardware later.",
          "Arrange the stage. Drag each light to where it is in the room. Patterns that sweep across the lights follow this layout.",
          "Make it yours. The panel on the right holds the scene that is playing. Add an effect, change its colours, then press \"Save as scene\".",
        ],
      },
      { head: "Where things are" },
      {
        list: [
          "Left: your lights and groups of lights.",
          "Middle: the stage, a live picture of what the lights are showing.",
          "Right: the scene, and the effects that make it up.",
          "Bottom: your scenes. Top: Edit, Live and Timeline views, the sound meter, brightness and Blackout.",
          "The gear opens Settings. \"Full layout\", at the top of the right panel, shows every control; the simple layout shows what most shows need.",
        ],
      },
    ],
  },
  {
    id: "lights",
    title: "Lights",
    blocks: [
      { head: "Adding lights" },
      {
        list: [
          "WLED devices on your network appear under \"Add a WLED device\". Press Add. If none appear, type the device's IP address, or scan the network.",
          "sACN and Art-Net pixel controllers: choose \"Add an sACN or Art-Net controller\" and give the address, the first universe and the number of LEDs.",
          "Virtual lights exist only on screen. Choose \"Add a virtual light\", pick a strip or a matrix and its size.",
          "Demo lights are a ready-made set of virtual lights. One switch adds or removes them.",
        ],
      },
      { head: "Planning before you have the hardware" },
      {
        steps: [
          "Add a virtual light for each strip or matrix you intend to put up, and name it.",
          "Drag them into place on the stage, bend strips into shape, group them, and build your scenes.",
          "When the real hardware is up, open the virtual light in the Lights panel and choose \"Connect to real hardware\". Pick the WLED device or give the controller's address.",
        ],
      },
      "The light keeps its place, its shape, its group and everything that plays on it. A single strip takes the number of LEDs the device really has.",
      { head: "On the stage" },
      {
        list: [
          "Drag a light to move it. Drag its round handle to turn it, or type an angle.",
          "Shift+click picks several; Shift+drag, or the Select button, draws a box. Double-click picks a light's whole group.",
          "Shape: bend a strip into an arch, circle, star, or a line of your own.",
          "Turning a matrix says how it hangs in the room. Patterns on it stay level.",
          "Fit brings everything back into view.",
        ],
      },
      { head: "Groups of lights" },
      "A group is a named set of lights, such as Roof or Stage left. Press \"New group\" at the top of the Lights panel and drag lights into it, or pick lights on the stage and choose Group. Any effect can then play on the group, and on lights you add to it later.",
      { head: "Each light's settings" },
      {
        list: [
          "Brightness limit: the most this light will ever show.",
          "In the full layout: gamma, a power limit in amps, a delay to line it up with slower lights, the colour order for sACN and Art-Net, and smoother dim levels.",
          "The switch beside a light stops sending to it without removing it.",
        ],
      },
    ],
  },
  {
    id: "scenes",
    title: "Scenes",
    blocks: [
      "A scene is a complete look: a set of effects with their settings. The panel on the right always shows one scene's effects, and its heading says which.",
      { head: "Making and saving" },
      {
        list: [
          "Change the effects on the right and the lights change at once.",
          "\"Save as scene\" keeps what is playing under a name. It gets a number key if one is free.",
          "After changing a scene, the heading reads \"changed since it was saved\". \"Save changes to…\" keeps the change; pressing the scene's button again throws it away.",
        ],
      },
      { head: "Changing a scene without playing it" },
      "Open the arrow beside a scene and choose \"Change it without playing it\", or use Change in the scene manager. The scene opens in the right panel and the stage shows it, while your real lights go on playing what they were. Press Done to keep the changes, or \"Put it on the lights\" to play it.",
      { head: "The scene manager" },
      "\"Manage scenes\", at the bottom left, opens it.",
      {
        list: [
          "In this show: play, change or delete any scene.",
          "Add a scene: twenty-five ready-made scenes, looks you have kept from other shows, or a new empty scene.",
          "Find a scene by typing part of its name.",
        ],
      },
      { head: "Sets of scenes" },
      "A set is a group of scenes for one occasion: dinner, a party, Christmas. Make sets in the scene manager and file scenes in them. Choose the set in use there or from the list at the bottom left. The scene buttons, the next and previous keys and autopilot keep to the set in use. A scene filed in \"Every set\" is always offered.",
      { head: "Autopilot" },
      "Autopilot changes scene by itself every few bars and when the song changes, choosing calm scenes for quiet parts and big ones for drops. Each scene's arrow has a switch to keep autopilot away from it.",
    ],
  },
  {
    id: "effects",
    title: "Effects",
    blocks: [
      "An effect is one thing happening on the lights: a spectrum, a pulse on the beat, falling snow. A scene is a stack of them; those higher in the list are drawn over those below.",
      { head: "Adding one" },
      "Press \"Add an effect\". Each tile shows what the effect looks like; point at one, or press its ⓘ, to see it move and read what it does. Sections fold from their headings, and the same button closes the list again.",
      { head: "Its settings" },
      {
        list: [
          "Click an effect to open it; click its heading again to close it.",
          "Colours: a built-in palette or one of your own.",
          "Plays on: all lights, only strips, only matrices, a group, or single lights. With several single lights chosen, \"Name these as a group\" makes a group of them.",
          "Each light on its own: off draws the effect once across all the lights; on gives every light the whole effect.",
          "Strength: how strongly it shows over what is below.",
          "The eye hides an effect without removing it.",
        ],
      },
      { head: "Tools for any pattern" },
      {
        list: [
          "Angle: turn the pattern, in steps of 45° with the two buttons.",
          "In the full layout: Spin, Mirror, Repeat, Kaleidoscope, a spread from light to light, and Colour shift.",
        ],
      },
      { head: "Sliders that follow the music" },
      "In the full layout, the small wave beside a slider ties it to something: loudness, bass, mids, highs, the beat, hits, the song's energy, a shape in time with the music, or a fader in the Live view. \"By how much\" sets the range and Smoothing calms it.",
      { head: "Folders" },
      "\"New folder\" makes a folder; drag effects onto it. A folder switches or dims everything in it at once, and \"Everything inside plays on\" points all its effects at the same lights. A folder can be kept through scene changes, for things you play by hand.",
      { head: "Listing by lights" },
      "\"By lights\", above the list, sorts the effects under the lights and groups they play on.",
    ],
  },
  {
    id: "sound",
    title: "Sound and the beat",
    blocks: [
      "darkRGB hears what the computer is playing. The meter at the top shows it; click it to choose another source, such as a microphone.",
      {
        list: [
          "It follows loudness, bass, mids and highs, single hits, the tempo, and how big this part of the song is.",
          "When the drums drop out, the tempo is kept from memory and picked up again when they return.",
          "Sound sensitivity, in the Live view and in Settings, makes everything react more or less.",
          "\"Follow the song's energy\" makes quiet parts dimmer and calmer.",
        ],
      },
      { head: "If the lights are late" },
      "Reactions to a sound cannot be early. Things timed to the beat can: \"Lights lead the beat by\", in Settings under Sound, runs them a little ahead so they land with the kick. It is off until you set it. If your speakers themselves are late (Bluetooth), use \"Light delay\" instead.",
      { head: "Correcting the beat" },
      "In the Live view: ÷2 and ×2 for half and double time, the arrows to nudge it earlier or later, Tap to tap it in, and Beat 1 on the first beat of a bar. \"Follow the music\" undoes them.",
    ],
  },
  {
    id: "live",
    title: "Playing live",
    blocks: [
      "The Live view is for performing: big pads for scenes, effects and actions, and the keyboard keys that go with them.",
      {
        list: [
          "A pad can switch its thing, act only while held, or fire an effect once.",
          "Customize sets the pads up: give a pad a key, change what a press does, add a pad for any effect, folder or group, or a ready-made flash, strobe or colour.",
          "Blackout, tap tempo, fire a beat, next and previous scene, and freeze all have pads and keys.",
          "Faders: speed, sound sensitivity, and four more that any slider can follow.",
          "Colour pads recolour everything that is playing. Pads can be spread over four pages.",
        ],
      },
      { head: "MIDI controllers" },
      "In Customize, choose MIDI on a pad and press the key or pad on your controller. Knobs and faders are set up in Settings under Live. A mixer or program that sends MIDI clock can drive the beat.",
      { head: "A phone as a remote" },
      "In Settings, switch on \"Let other devices connect\", open the address shown on the phone, and type the six-digit code.",
    ],
  },
  {
    id: "timeline",
    title: "Timeline",
    blocks: [
      "The Timeline view lays a show out against a song.",
      {
        steps: [
          "Choose a song. Its outline is drawn and its tempo guessed for a beat grid.",
          "Click the Scenes lane to say which scene starts where.",
          "Drag in a pad's lane to hold that pad there: a flash, a strobe, a colour.",
          "Or press Record and Play, then perform. What you play is written down to tidy afterwards.",
        ],
      },
      "Clips move, stretch, snap to the beat and delete. A timeline can repeat.",
    ],
  },
  {
    id: "safety",
    title: "Flashing and safety",
    blocks: [
      "Fast flashing can trigger seizures in people with photosensitive epilepsy. Each show has a flash limit that holds flashing to three a second. A new show asks you to choose the first time something flashes fast; a live show starts with the limit off. With it off and other people present, the risk is yours to manage.",
      {
        list: [
          "The limit is in Settings, and as a switch in the Live view.",
          "Maximum brightness, in Settings, caps everything. Each light also has its own limit and, in the full layout, a power limit in amps.",
          "The power limit is an estimate for 5 volt strips. Set it a little under what your power supply gives.",
        ],
      },
    ],
  },
  {
    id: "shows",
    title: "Shows and saving",
    blocks: [
      "Everything saves itself as you go. Undo and redo are the arrows by the show's name, or Ctrl+Z and Ctrl+Shift+Z.",
      {
        list: [
          "Click the show's name for your library: open, start, copy, rename and delete shows.",
          "Effects, looks and palettes you keep are there too, to use in any show.",
          "A show can keep hours: lit from one time of day to another.",
        ],
      },
    ],
  },
  {
    id: "trouble",
    title: "If something is wrong",
    blocks: [
      {
        list: [
          "No sound: the meter at the top says so. Click it and choose the source. On Linux, sound must be playing through PulseAudio or PipeWire.",
          "A WLED device is not found: type its IP address, or scan. It must be on the same network as this computer.",
          "A light is added but dark: check its switch in the Lights panel, the Blackout button, and the brightness at the top. In WLED, another program sending to the device at the same time will make it flicker.",
          "Colours are wrong on an sACN or Art-Net controller: set its colour order in the light's settings (full layout).",
          "The beat is at half or double speed: use ×2 or ÷2 in the Live view.",
          "Lights land after the beat: see \"If the lights are late\" under Sound and the beat.",
        ],
      },
    ],
  },
];

const textOf = (topic: Topic): string =>
  [topic.title, ...topic.blocks.flatMap((b) => (typeof b === "string" ? [b] : "steps" in b ? b.steps : "list" in b ? b.list : [b.head]))]
    .join(" ")
    .toLowerCase();

/** The button that opens help. */
export function HelpButton() {
  return (
    <button type="button" className="icon-button" aria-label="Help" title="Help: a quick start, and how to use each part" onClick={() => helpTopic.set("start")}>
      <CircleHelp size={17} />
    </button>
  );
}

export function Help() {
  const open = useSyncExternalStore(helpTopic.subscribe, helpTopic.get);
  const [search, setSearch] = useState("");
  useEffect(() => {
    if (open === null) return;
    const close = (e: KeyboardEvent) => e.key === "Escape" && helpTopic.set(null);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [open]);
  if (open === null) return null;
  const words = search.trim().toLowerCase();
  const found = words === "" ? TOPICS : TOPICS.filter((t) => textOf(t).includes(words));
  const topic = found.find((t) => t.id === open) ?? found[0];
  return (
    <div
      className="dialog-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) helpTopic.set(null);
      }}
    >
      <div className="dialog settings help" role="dialog" aria-modal="true" aria-label="Help">
        <nav className="settings-nav" aria-label="Help topics">
          <h2>Help</h2>
          <label className="picker-search">
            <Search size={13} aria-hidden />
            <input
              type="search"
              placeholder="Search help"
              aria-label="Search help"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => e.key !== "Escape" && e.stopPropagation()}
            />
          </label>
          {found.map((t) => (
            <button key={t.id} type="button" className={t.id === topic?.id ? "selected" : ""} aria-current={t.id === topic?.id} onClick={() => helpTopic.set(t.id)}>
              {t.title}
            </button>
          ))}
          {found.length === 0 && <p className="hint">Nothing about that.</p>}
        </nav>
        <div className="settings-page">
          <div className="settings-head">
            <h2>{topic?.title ?? "Help"}</h2>
            <button type="button" className="icon-button" aria-label="Close help" onClick={() => helpTopic.set(null)}>
              <X size={16} />
            </button>
          </div>
          <div className="settings-body help-body">
            {topic?.blocks.map((block, i) =>
              typeof block === "string" ? (
                <p key={i}>{block}</p>
              ) : "head" in block ? (
                <h3 key={i}>{block.head}</h3>
              ) : "steps" in block ? (
                <ol key={i}>
                  {block.steps.map((step) => (
                    <li key={step}>{step}</li>
                  ))}
                </ol>
              ) : (
                <ul key={i}>
                  {block.list.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              ),
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
