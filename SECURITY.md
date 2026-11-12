# Security

## What the program exposes

- darkRGB runs a small web server for its own interface. By default it listens only on the computer it runs
  on (127.0.0.1, port 7420). It checks the `Host` and `Origin` headers, so a web page you visit cannot talk
  to it.
- Started with `--lan`, it listens on your network so a phone can open the interface. **There is no password
  yet.** Anyone on that network can then control the lights and change the open show. Use `--lan` only on a
  network you trust.
- It sends lighting data to the devices you add, over UDP (DDP), and asks them for their details over HTTP.
  It looks for WLED devices with mDNS, and scans a network range only when you ask it to.
- It records nothing. Sound is analysed in memory and never written or sent anywhere.
- Shows are stored as plain JSON files in your user configuration folder. A show file from someone else is
  checked like any other input before it is opened.

## Reporting a problem

Please do not open a public issue for something that could be misused. Use GitHub's "Report a vulnerability"
button on the repository's Security tab, which reaches the maintainer privately. If that is not available,
open an issue that says only that you have a security report, and the maintainer will arrange a private way
to receive it.

There is no bounty. Reports are read and answered as time allows.

## Flashing lights

Fast flashing can trigger seizures in people with photosensitive epilepsy. Each show has a flash limit; see
the README. A way to make the lights flash fast while a show's limit is on is a bug worth reporting, in the
open.
