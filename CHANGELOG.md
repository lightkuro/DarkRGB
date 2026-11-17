# What's new

## 0.11.0 (in development)

Closing the gap with LedFx, and more ways to play a show live. What was taken from LedFx, what was changed
on the way and what was left out is listed in [docs/research.md](docs/research.md).

Scenes and playing live

- **Next.** A Next button, with Previous, in the scenes bar and in the Live view. It can go to the next
  scene in the list, to the next of the same kind, or to one that suits the part of the song playing
  right now. The small arrow beside it chooses, and can jump straight to the next scene for quiet,
  in-between or big parts.
- What a scene suits is set in its menu: **Any part**, **Quiet parts**, **In between** (new) or **Big
  parts**. Autopilot goes by the same.
- **Discard.** A scene changed without playing it can be put back as it was when you opened it.
- Number keys can play the scenes of the set in use by their place (key 1 the first, key 2 the second)
  instead of each scene keeping a key of its own. Settings, Live and keys; offered once you have sets.
- Seven ways for one scene to give way to the next: a fade, brighten, through black, through white, a
  wipe across the stage, an iris from its middle, and a dissolve. Settings, Live and keys, once scene
  changes fade.
- The Live view is made of panels. Under **Customize** each can be moved earlier or later or hidden,
  and hidden ones added back. New panels: **Next scene**, and **The song right now** (tempo, how big this
  part is, and the note). The faders and switches are a panel too.
- Pads can be put in named groups: pick a pad under Customize and type a group's name.

Effects

- Nine new effects. **Take turns**: halves of a light, or whole lights, flashing one after another.
  **Stage beams**: beams that sway, cross and fan like moving heads. **Note colours**: the colour follows
  the tune. **Waterfall**: the sound's low to high notes falling down a matrix. **Glitch**, **Lava lamp**,
  **Code rain**, **Life**, and **Moving picture**.
- **Moving picture** plays a GIF or a piece of video on a matrix, at its own speed or one showing to so
  many beats. Choose the file where the effect's settings are. Video is read by the page itself, so any
  file your browser plays will do; up to the first minute is kept, without sound.
- darkRGB now hears the note that stands out in the music. Any slider can follow it ("The note being
  played"), so any effect's hue can follow the tune.
- **Blur** softens any effect (full layout).

Lights

- Zones can be cut by percent as well as by numbers of LEDs, with an "Ends and middle" shortcut.
- A matrix's wiring can be set up in WLED or here. A matrix that WLED reports is left to WLED; choose
  "Here, in darkRGB" to describe it yourself and keep WLED as one plain strip.
- A computer's own lights: keyboards, memory, fans and the like, through the free OpenRGB program. In the
  Lights panel choose **Add a computer's own lights (OpenRGB)**. Written from OpenRGB's published
  protocol and checked against a stand-in for it; it has not met OpenRGB itself yet.

## 0.10.0

More ways to say what your lights are and how a pattern lies on them. Several came from a comparison with
LedFx; see [docs/research.md](docs/research.md).

- Zones. A strip can be cut into parts that are lights of their own, so the ends can play one effect and
  the middle another, each listening to a different part of the sound. Open the strip in the Lights panel
  and choose **Cut into zones**; **Make it one strip again** undoes it. The LEDs stay where they were on
  the stage, and whatever played on the strip plays on all its zones.
- A strip snaked into a matrix. Open the strip and choose **It is snaked into a matrix**, then say how
  many LEDs are in a row.
- Matrix wiring. Say which corner the first LED is in, whether the LEDs run along the rows or up and down
  the columns, and whether they snake. A small drawing shows the order.
- Three ways to lay an effect on several lights (full layout): **Across the stage**, by where the lights
  are; **On each light**; or **End to end**, one line through every LED in the order they are wired.
- **Blocks**: a pattern made of a few blocks, so several LEDs act as one. **Slide along**: move a pattern
  along its lights. Both in the full layout, for any effect.
- A folder can give all its effects the same part of the sound: "Everything inside listens to".
- A matrix made from a strip is put below the other lights, not on top of them.

## 0.9.0

Easier to find your way round: scenes you can see and manage, effects you can browse, lights you can plan
before they exist, and help built in.

- Help. The **?** at the top opens a quick start and a guide to each part of darkRGB, with a search box.
- Plan your lights before the hardware exists. Add virtual strips and matrices (now in the simple layout
  too), lay them out, group them and build scenes. When the real thing is up, open the light and choose
  **Connect to real hardware**: it keeps its place, shape, group, name and everything that plays on it.
  A single strip takes the number of LEDs the device really has.
- Effects are chosen from tiles, each with a small moving picture of the effect. Point at one, or press
  its ⓘ, to see it larger and read what it does. Sections fold, show six until asked for all, and there
  is a search box.
- The right panel now says which scene it holds: playing, changed since it was saved, or not a scene yet.
  **Save changes** and **Save as a new scene** are there.
- Change a scene without playing it. The scene opens in the panel and on the stage while your lights go on
  with what they were playing. **Done** keeps the changes; **Put it on the lights** plays it.
- A scene manager (**Manage scenes**, bottom left): every scene in the show, ten new ready-made scenes
  (twenty-five in all) to add one at a time, looks you kept from other shows, and new empty scenes.
- Sets of scenes. File scenes in named sets, such as Dinner and Party, and choose the set in use. The
  scene buttons, the next and previous keys and autopilot keep to it.
- The list of effects can be shown **By lights**: under the lights and groups each effect plays on.
- Groups of lights are easier to make: a labelled **New group** button in both layouts, and "Name these as
  a group" when an effect plays on several single lights.
- Folders of effects are easier to find: a labelled **New folder** button in both layouts. A folder can
  now point everything in it at the same lights in one go.
- Things close where they opened. The button that opens a list or a form stays where it is and closes it
  again, and the heading of an open effect or light stays in view while its settings scroll.
- "Lights lead the beat" is off unless you set it, and its text says what it can and cannot move: only
  things timed to the beat, never a reaction to a sound.
- Fixed: starting a live show opened its Live folder's settings by itself.

## 0.8.0

A beat that holds, tools that work on any pattern, and outputs that look after the hardware. Most of this
round answers an outside review of 0.6.0; see [docs/roadmap.md](docs/roadmap.md).

- The beat keeps going. When the drums drop out for a breakdown the tempo is held from memory, for up to
  sixteen bars, and picked up again when they return. The pause before a drop is counted through.
- Bars have a first beat. Four boxes in the Live view show where the bar is; press **Beat 1** on the first
  beat of a bar and scene changes and autopilot follow it.
- Corrections that keep the music's tempo: half time and double time, a nudge earlier or later, and tapping,
  which now adjusts the music's own beat when your taps agree with it. **Follow the music** undoes them all.
- Lights fire a little early, 50 ms by default, so they land with the sound and not after it. The setting
  is "Lights lead the beat" in Settings.
- MIDI clock: a DJ mixer or a music program that sends clock can drive the beat exactly.
- Any pattern can be mirrored, repeated, wrapped round its middle as a kaleidoscope, staggered from one
  light to the next, and have its colours turned round the colour wheel. The colour shift can follow the
  music.
- The angle of a pattern moves in steps of 45° in the simple layout, with two buttons that turn it an
  eighth of a turn either way. The full layout keeps the fine slider.
- Links have smoothing, and three more things to follow, all in time with the music: a climb, on and off,
  and a new level by chance.
- Beat pulse and Sweep can take their timing from the tempo.
- A power limit for each device, in amps. When a picture would draw more, the whole frame is dimmed alike.
  The device's settings show what the lights are drawing now. It is an estimate for 5 volt strips.
- sACN and Art-Net controllers can be told the order their colours go in.
- A delay for each device, to line a quick one up with a slow one.
- Smoother dim levels, as a switch on each device: slow fades near black move in finer steps. Off unless you
  switch it on, because it has not been seen on real LEDs yet.
- Closing darkRGB leaves the lights dark, and tells sACN controllers the stream has ended. Before, a pixel
  controller held its last frame.
- If the part that draws the frames ever fails, it starts again by itself and says so.
- Live shows no longer dim quiet parts of a song by default.
- Fixed: a short silence inside a song made the next loud part read as nothing special.
- Fixed: a full panel could squeeze its contents over each other.
- Fixed: Fit left the lowest light under the stage's buttons.

## 0.7.0

A timeline, shapes of your own, patterns that turn, and eight more effects.

- A timeline, in a third view beside Edit and Live. Choose a song; its outline is drawn and its tempo guessed
  for a beat grid. Click the Scenes lane to say which scene starts where. Drag in a pad's lane to hold that
  pad there: a flash, a strobe, a colour, a blackout. Clips move, stretch, snap to the beat and delete.
- Record a performance: press Record and Play, then choose scenes and press pads as the song runs. Everything
  is written onto the timeline to tidy up afterwards. Play and Record are in the Live view too.
- A timeline can repeat, for a show left running.
- Strips can be bent into shapes. Pick an arch, circle, square, triangle, star, zigzag or spiral, or draw
  your own corner by corner. Select a strip and choose Shape.
- Patterns can be turned: every effect has an Angle, and in the full layout a Spin. The angle can follow the
  music or a fader.
- A matrix hung at an angle, such as a square mounted as a diamond, now keeps its pattern the right way up.
  Turn the matrix on the stage to match how it hangs. (A switch puts back the old behaviour, where the
  pattern turned with the light.)
- Eight new effects, thirty in all: Scanner, Marquee, Breathe, Pinwheel, Peaks (bars with caps that hang
  and fall), Fireworks, Rings and Tiles. Most can keep time with the music.
- Fixed: lights flickered while being dragged on the stage.
- Fixed: a jump on the timeline while stopped did not show until playback started.

## 0.6.0

Controllers, a phone as a remote, words and pictures, more kinds of lights, and installers.

- The kinds of show are now called Light show, Live show, Room show and Blank.
- MIDI controllers. Any pad can learn a note: choose "MIDI" on the pad and press the key or pad on your
  controller. Knobs and faders can drive brightness, speed, sensitivity and the four live faders. Not yet
  tried with a real controller; please report how yours behaves.
- Undo and redo for every change to a show, with Ctrl+Z and Ctrl+Shift+Z or the arrows by the show's name.
  Dragging a slider is one step.
- Colour pads: one press recolours everything that is playing, and another gives the colours back.
- Pads can be spread over four pages, so one key or one controller pad can do four things. An XY pad bends
  two settings at once with one finger.
- Words: a line of text across a matrix, still or scrolling.
- Picture: any image, shrunk to fit your matrix. Simple pictures with strong colours look best.
- A show can keep hours, such as on at five in the afternoon and off at half past ten.
- Pixel controllers that take sACN (E1.31) or Art-Net can be added by address and universe, beside WLED
  devices. Not yet tried with a real controller.
- A phone or tablet as a remote: switch it on in Settings, open the address shown on the phone, and enter
  the six-digit code. Before, opening the app to the network let anyone on it control the lights.
- Installers: a setup program for Windows, a `.deb` and an AppImage for Linux. Every download now carries
  the licences of the libraries built into it.
- Fixed: answers to some commands never reached the part of the app waiting for them, so "Use" on a kept
  effect gave no confirmation.

## 0.5.0

A show for every kind of place, a simpler screen to start on, and sliders that move with the music.

- Starting a show asks what you are lighting: a room, a light show for a house or garden, or a club night.
  Each begins with scenes, pads and settings of its own. Or start blank.
- Eight new starter scenes, fifteen in all: Aurora, Fairy lights, Candy cane, Snowfall, Red and green, Warm
  glow, Colour hits and Storm.
- Five new effects: Stripes, Twinkle, Snow, Colour steps and Lightning. Six new palettes.
- The flash limit now belongs to each show. The first time something in a show flashes fast, you are asked
  whether to keep the limit or switch it off; after that a small note on the stage is all there is. A club
  show starts with it off.
- With the limit on, a strobe slows itself to three clean flashes a second instead of being softened, and
  stays in time with the music. Lightning, which only works flashing fast, is switched off and says so.
- A simple layout and a full one. Simple, which you start in, shows what most shows need. Full adds folders,
  groups, what an effect hears, how effects mix, keys on everything and every setting. One switch, in
  Settings or above the effects.
- Any slider can follow the music: loudness, bass, mids, highs, the beat, hits or the song's energy. Or a slow
  wave in time with the music, or one of four faders in the Live view.
- Palettes of your own: two to eight colours, picked in the colours list.
- Keep an effect with its settings, a whole look, or a palette, and use it in any show. Kept things are
  offered first when you add an effect, and listed under the show's name.
- When the music stops, the lights can change to a scene you choose and come back when it starts again.
- Autopilot picks calm scenes for the quiet parts of a song and big ones for the drops. Each scene can say
  which it is.
- An effect can be kept to strips or to matrices, whichever lights the show has now or gets later.

## 0.4.0

Live mode you can set up as you play, lights that follow the shape of the song, and a place for your shows.

- Flash limit: hard flashes faster than three a second are softened into a swell, on the lights and in the
  preview alike. One switch in Settings or the Live view turns it off, and a strict level is there for more
  care.
- Pads in the Live view can be set up without leaving it: Customize, click a pad, press a key. A pad no
  longer needs a key, so a touch screen can have pads of its own.
- "Fire it": a third thing a key can do. Each press sets off the effect's reaction as if a beat had hit, music
  or not. An effect can also be told to react only when fired.
- Hold a scene's key to show that scene only while it is held. New keys: next scene, previous scene, freeze
  the picture.
- Ready-made pads: a white flash, a strobe or a colour pulse in one click. They live in a pinned folder that
  stays put when the scene changes.
- Speed and sound sensitivity as faders in the Live view.
- Pick lights on the stage: click, Shift and click, drag a box, double-click for a whole group. Several lights
  move and turn together, and an angle can be typed.
- The lights follow the shape of the song: quiet parts are dimmer and calmer, drops hit harder. "Follow the
  song's energy" in Settings sets how much.
- Autopilot changes scene every few bars, and at once when the song changes. A scene can be told to sit out.
- Seven starter scenes in one click. "Try it with demo lights" adds them and starts autopilot.
- Five new effects: Strobe, Sweep, Chase, Fire and Meteors. Strobe and Chase can lock to the tempo.
- "Each light on its own": an effect drawn once per light instead of once across all of them.
- Shows: the show's name opens a list of your shows. Open, start new, copy, rename, delete (to a trash
  folder), or bring one in from a file. Nothing is replaced when you start or open another.
- 120 frames a second, with a note on what each of your lights can keep up with.

## 0.3.0

Playing the lights from the keyboard, and keeping a growing show tidy.

- Keys: give a scene, an effect, a folder of effects or a group of lights a key by pressing it. A key can
  switch its thing on each press, or only while it is held down, which is how to flash.
- The first ten scenes get the number keys by themselves.
- A Live view: a big stage with a pad for every scene and every key. Pads work by mouse and touch too.
- Scene changes can wait for the beat, and can fade instead of cutting.
- Tap the tempo when the music's beat is not being found, and fire a beat by hand.
- Folders for effects: drag effects in, fold the folder shut, switch or dim everything inside at once.
- Groups of lights: an effect can play on a whole group, and a group can be dimmed or switched off as one.
- Settings, behind the gear button: a brightness ceiling, frames per second, a delay for speakers that play
  late, the keys, and the show file.
- Save a copy of the show, open a show file, or start a new show and keep your lights.
- Scenes can be renamed and updated from their own menu.

## 0.2.0

New name: darkRGB.

- Opens as one app window. No browser tab, no terminal.
- Demo lights: switch on two strips and a matrix that exist only on screen, to try effects without hardware.
- Virtual lights: add strips and matrices of any size to plan a layout before wiring anything.
- Adding a device is quicker: the address field starts with your network's prefix, and "Scan" asks every
  address on that network for WLED devices when automatic discovery finds nothing.
- Effects collapse to one row each. Drag to reorder, rename and duplicate them.
- Each effect can react to everything, bass, mids, highs, or a frequency range you pick on a live spectrum.
- Any effect's brightness can follow the sound, including effects that otherwise ignore it.
- An Advanced switch reveals the less common settings; they stay out of the way until then.
- Effects respond more: steady sound reads low and hits stand out.

## 0.1.0

First working slice: lights that follow the music.

- Finds WLED devices on your network and adds them in one click, or by IP address.
- Captures whatever the computer is playing with no audio setup.
- Ten effects with live controls, stackable in layers.
- A live preview where you arrange strips and matrices by dragging.
- Scenes to save and recall a look.
- Master brightness and blackout.
