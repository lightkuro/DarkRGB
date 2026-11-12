#!/usr/bin/env python3
"""Writes the licence notices of everything built into darkRGB's programs.

    python3 tools/third_party.py THIRD-PARTY-LICENSES.txt

The programs contain Rust libraries (compiled in) and JavaScript libraries
(bundled into the UI). Their licences ask that the notice travel with any
copy, so this file is made for every build and shipped beside the
programs. It lists what is built for the machine it runs on: a Windows
build and a Linux build contain different libraries.

It needs nothing but Python, `cargo`, and the UI's dependencies having
been installed (`pnpm install` in ui/).
"""

import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# The programs that are handed out.
PROGRAMS = ["darkrgb-desktop", "darkrgb-engine"]
# File names a licence or notice is kept under, in lower case.
STARTS = ("license", "licence", "copying", "notice", "unlicense", "copyright")
# One licence text is this long at most; anything longer is not one.
MAX_BYTES = 200_000


def notices(folder: pathlib.Path) -> list[tuple[str, str]]:
    """The licence files in a package's folder, as (name, text)."""
    found = []
    try:
        entries = sorted(folder.iterdir(), key=lambda p: p.name.lower())
    except OSError:
        return found
    for path in entries:
        if not path.is_file() or not path.name.lower().startswith(STARTS):
            continue
        if path.stat().st_size > MAX_BYTES:
            continue
        text = path.read_text(encoding="utf-8", errors="replace").strip()
        if text:
            found.append((path.name, text))
    return found


def host_triple() -> str:
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return next(line.split(": ", 1)[1] for line in out.splitlines() if line.startswith("host: "))


def rust_packages() -> list[dict]:
    """Every crate compiled into the programs for this machine."""
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", host_triple()],
        capture_output=True, text=True, check=True, cwd=ROOT,
    ).stdout
    meta = json.loads(out)
    by_id = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    ours = set(meta["workspace_members"])
    todo = [p["id"] for p in meta["packages"] if p["name"] in PROGRAMS and p["id"] in ours]
    if len(todo) != len(PROGRAMS):
        sys.exit("could not find the programs in the workspace")
    seen = set()
    while todo:
        pid = todo.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            # Libraries used only to build or to test are not in the programs.
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    packages = []
    for pid in seen - ours:
        p = by_id[pid]
        packages.append({
            "name": p["name"],
            "version": p["version"],
            "license": p.get("license") or "see the files below",
            "home": p.get("repository") or p.get("homepage") or f"https://crates.io/crates/{p['name']}",
            "files": notices(pathlib.Path(p["manifest_path"]).parent),
        })
    return sorted(packages, key=lambda p: (p["name"].lower(), p["version"]))


def js_packages() -> list[dict]:
    """Every JavaScript package bundled into the UI."""
    modules = ROOT / "ui" / "node_modules"
    wanted = list(json.loads((ROOT / "ui" / "package.json").read_text())["dependencies"])
    if not modules.is_dir():
        sys.exit("ui/node_modules is missing: run `pnpm install` in ui/ first")
    seen, packages = set(), []
    todo = [(name, modules) for name in wanted]
    while todo:
        name, near = todo.pop()
        # Look beside the package that asked, then further up, as Node does.
        folder = None
        for base in [near, *near.parents]:
            candidate = base / name if base.name == "node_modules" else base / "node_modules" / name
            if (candidate / "package.json").is_file():
                folder = candidate.resolve()
                break
        if folder is None:
            sys.exit(f"could not find the JavaScript package {name}")
        info = json.loads((folder / "package.json").read_text(encoding="utf-8"))
        key = (info["name"], info["version"])
        if key in seen:
            continue
        seen.add(key)
        repo = info.get("repository")
        home = repo.get("url") if isinstance(repo, dict) else repo
        packages.append({
            "name": info["name"],
            "version": info["version"],
            "license": info.get("license") or "see the files below",
            "home": home or info.get("homepage") or f"https://www.npmjs.com/package/{info['name']}",
            "files": notices(folder),
        })
        for dep in info.get("dependencies", {}):
            todo.append((dep, folder.parent))
    return sorted(packages, key=lambda p: p["name"].lower())


def write(out: pathlib.Path) -> None:
    rust, js = rust_packages(), js_packages()
    lines = [
        "darkRGB: licences of the software built into it",
        "=" * 47,
        "",
        "darkRGB itself is free software under the GNU General Public License,",
        "version 3 or later (see the LICENSE file beside this one).",
        "",
        "Its programs also contain the libraries listed here, each under its own",
        "licence. This file was made when the programs were built, for the kind",
        f"of computer they were built for ({host_triple()}).",
        "",
        f"{len(rust)} Rust libraries and {len(js)} JavaScript libraries:",
        "",
    ]
    for p in js + rust:
        lines.append(f"  {p['name']} {p['version']}  ({p['license']})")
    # Hundreds of libraries carry the same text word for word (the Apache
    # licence above all). Each text is printed once and referred to by number.
    texts: dict[str, int] = {}
    missing = []
    for title, packages in (("JavaScript libraries, in the UI", js), ("Rust libraries", rust)):
        lines += ["", "", title, "-" * len(title)]
        for p in packages:
            lines += ["", f"{p['name']} {p['version']}", f"  Licence: {p['license']}", f"  {p['home']}"]
            if not p["files"]:
                missing.append(f"{p['name']} {p['version']}")
                lines.append("  The package carries no licence file of its own. Its licence is named above;")
                lines.append("  the full text is at https://spdx.org/licenses/ under that name.")
            for name, text in p["files"]:
                number = texts.setdefault(text, len(texts) + 1)
                lines.append(f"  {name}: text {number} below")
    title = "The texts"
    lines += ["", "", title, "-" * len(title)]
    for text, number in texts.items():
        lines += ["", "", "=" * 78, f"Text {number}", "=" * 78, "", text]
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {out}: {len(rust)} Rust and {len(js)} JavaScript libraries, {out.stat().st_size // 1024} KB")
    if missing:
        print(f"{len(missing)} carry no licence file of their own: {', '.join(missing)}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    write(pathlib.Path(sys.argv[1]))
