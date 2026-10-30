"""Read your finished umas (the game's Veteran List) out of the running game, on Linux.

The game has no export, but the server's reply for the Veteran List stays in the game's memory as
msgpack. This finds it and writes it as data.json, the same format as UmaExtractor
(github.com/xancia/UmaExtractor, the Windows tool), for umaLord's Settings → Profile → Import
finished umas (or `umalord --import-veterans data.json`). It only reads /proc/<pid>/maps and
/proc/<pid>/mem: it never writes to the game. Account ids (yours, and other players' on borrowed
parents) are removed.

1. Start the game (Steam with Proton) and open Enhance → Veteran List, so the list is loaded.
2. Run, as root when kernel.yama.ptrace_scope is 1 or more (most distributions):
       sudo python3 -I tools/read_veterans.py [-o ~/Downloads/data.json]
   Root's Python needs the msgpack package (pip install msgpack). With Nix, build a Python that
   has it and run that one under sudo:
       PY=$(nix build --no-link --print-out-paths --impure --expr \\
           'with import <nixpkgs> {}; python3.withPackages (p: [p.msgpack])')/bin/python3
       sudo $PY -I tools/read_veterans.py -o ~/downloads/data.json
"""
import json
import os
import sys

import msgpack

KEY = b"\xb3trained_chara_array"  # msgpack fixstr(19) + the key
EXE = "umamusumeprettyderby.exe"


def rss(pid: int) -> int:
    try:
        line = next(l for l in open(f"/proc/{pid}/status") if l.startswith("VmRSS:"))
        return int(line.split()[1])
    except (OSError, StopIteration, ValueError):
        return 0


def game_pid() -> int:
    """The game's process; with several (a launcher stub and the game), the one using the most
    memory."""
    found = []
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            argv0 = open(f"/proc/{d}/cmdline", "rb").read().split(b"\0", 1)[0].decode("utf-8", "replace")
        except OSError:
            continue
        if argv0.replace("\\", "/").lower().endswith(EXE):
            found.append(int(d))
    if not found:
        sys.exit("The game isn't running (looked for UmamusumePrettyDerby.exe).")
    pid = max(found, key=rss)
    if len(found) > 1:
        print(f"Several game processes ({', '.join(map(str, found))}); reading {pid}, the largest.")
    return pid


def regions(pid: int):
    for line in open(f"/proc/{pid}/maps"):
        span, perms = line.split()[:2]
        start, end = (int(x, 16) for x in span.split("-"))
        if "rw" in perms and 16 * 1024 <= end - start <= 1024 * 1024 * 1024:
            yield start, end


def scrub(v):
    if isinstance(v, dict):
        v.pop("viewer_id", None)
        v.pop("owner_viewer_id", None)
        for x in v.values():
            scrub(x)
    elif isinstance(v, list):
        for x in v:
            scrub(x)


def owner(v: list):
    """The one account a list belongs to, or None if it mixes accounts or names none."""
    ids = {c.get("viewer_id", c.get("owner_viewer_id")) for c in v}
    return next(iter(ids)) if len(ids) == 1 and None not in ids else None


def read(pid: int) -> list:
    """The current Veteran List: older copies can linger in memory, so of the lists that belong
    to one account, the one holding the most recently finished uma (then the longest). Lists
    from more than one account are refused rather than guessed between."""
    lists: list = []
    try:
        mem = open(f"/proc/{pid}/mem", "rb")
    except PermissionError:
        sys.exit("Permission denied reading the game's memory: run with sudo.")
    with mem:
        for start, end in regions(pid):
            try:
                mem.seek(start)
                buf = mem.read(end - start)
            except OSError:
                continue
            i = buf.find(KEY)
            while i != -1:
                u = msgpack.Unpacker(raw=False, strict_map_key=False, max_buffer_size=64 * 1024 * 1024)
                u.feed(buf[i + len(KEY):i + len(KEY) + 32 * 1024 * 1024])
                try:
                    v = u.unpack()
                except Exception:
                    v = None
                if isinstance(v, list) and v and all(isinstance(c, dict) and "card_id" in c for c in v) and owner(v) is not None:
                    lists.append(v)
                i = buf.find(KEY, i + 1)
    if len({owner(v) for v in lists}) > 1:
        sys.exit("Veteran Lists from more than one account are in memory; restart the game, open your Veteran List, and run this again.")
    return max(lists, key=lambda v: (max(str(c.get("create_time", "")) for c in v), len(v)), default=[])


def main() -> None:
    out = sys.argv[sys.argv.index("-o") + 1] if "-o" in sys.argv else "data.json"
    out = os.path.expanduser(out)
    if os.environ.get("SUDO_USER") and out.startswith("/root"):
        sys.exit("Under sudo, ~ is root's home: give -o a full path.")
    pid = game_pid()
    vets = read(pid)
    if not vets:
        sys.exit("No Veteran List in the game's memory: open Enhance → Veteran List, wait for it to load, and run this again.")
    seen, kept = set(), []
    for c in vets:
        if c.get("trained_chara_id") in seen:
            continue
        seen.add(c.get("trained_chara_id"))
        scrub(c)
        kept.append(c)
    # As root, never follow a symlink someone left at the path, and hand the file back to the
    # user who ran sudo through the open file rather than the path.
    fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o644)
    if os.environ.get("SUDO_UID"):
        os.fchown(fd, int(os.environ["SUDO_UID"]), int(os.environ.get("SUDO_GID", os.environ["SUDO_UID"])))
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        json.dump(kept, f, ensure_ascii=False, indent=1)
    print(f"Wrote {len(kept)} finished umas to {out}.")


if __name__ == "__main__":
    main()
