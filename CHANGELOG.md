# What's new

## 0.3.0 (in development)

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
