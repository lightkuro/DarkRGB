# Security

## What the program exposes

- darkRGB runs a small web server for its own interface. By default it listens only on the computer it runs
  on (127.0.0.1, port 7420). It checks the `Host` and `Origin` headers, so a web page you visit cannot talk
  to it.
- With phone access switched on (in Settings, or by starting with `--lan`), it listens on your network so a
  phone can open the interface. Another device must first give a six-digit code shown on the computer; it
  is then handed a cookie and not asked again until the program restarts, when the code changes. Five wrong
  codes make that device wait a minute, and thirty from all devices together make everyone wait.
- The connection is plain HTTP, not encrypted: someone who can watch the traffic on that network could copy
  the cookie. The code keeps out people who are merely on the same network; it is not protection on a
  network you do not trust. Phone access is off unless you switch it on.
- A paired device can do everything the computer can with the show, except see the code or switch phone
  access on or off.
- It sends lighting data to the devices you add, over UDP (DDP, sACN or Art-Net), and asks WLED devices for
  their details over HTTP.
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
