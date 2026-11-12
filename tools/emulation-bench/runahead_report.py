#!/usr/bin/env python3
"""Reads a runahead.sh results folder and prints, for every button event, on
which shown frame the picture first changed (frames after the press), in
slot. and RetroArch with Run-Ahead 0, 1 and 2.

usage: runahead_report.py OUT_DIR [--json FILE]
"""
import json
import os
import sys

NAMES = ["B", "Y", "SELECT", "START", "UP", "DOWN", "LEFT", "RIGHT", "A", "X", "L", "R"]


def shown(log):
    """The picture hash of every shown frame, in order."""
    seq, cycle = [], None
    with open(log) as f:
        for line in f:
            e = json.loads(line)
            ev = e.get("event")
            if ev == "present":  # slot.: the test marks each shown picture
                seq.append(e["video"])
            elif ev == "frame" and "real" in e and not any_present[0]:
                # RetroArch: a real frame starts a shown frame; with Run-Ahead
                # the last hidden frame after it is the one shown.
                if e["real"]:
                    if cycle is not None:
                        seq.append(cycle)
                cycle = e["video"]
    if cycle is not None and not any_present[0]:
        seq.append(cycle)
    return seq


any_present = [False]


def load(path):
    with open(path) as f:
        any_present[0] = '"event":"present"' in f.read()
    return shown(path)


def mask_name(m):
    return "+".join(n for i, n in enumerate(NAMES) if m >> i & 1) or "release"


def main():
    out = sys.argv[1]
    events = [tuple(map(int, l.split())) for l in open(os.path.join(out, "events.txt")) if l.strip()]
    configs = [(fe, n) for fe in ("slot", "retroarch") for n in (0, 1, 2)]
    result = {"events": [], "configs": [f"{fe}-ra{n}" for fe, n in configs]}
    seqs = {}
    for fe, n in configs:
        d = os.path.join(out, f"{fe}-ra{n}")
        seqs[(fe, n)] = [load(os.path.join(d, str(k), "log.jsonl")) for k in range(len(events) + 1)]
    for k, (frame, mask) in enumerate(events):
        row = {"event": k, "frame": frame, "buttons": mask_name(mask), "lag": {}}
        for fe, n in configs:
            a, b = seqs[(fe, n)][k], seqs[(fe, n)][k + 1]
            t = next((i for i in range(min(len(a), len(b))) if a[i] != b[i]), None)
            row["lag"][f"{fe}-ra{n}"] = None if t is None else t - frame
        result["events"].append(row)
    # Without any button, does Run-Ahead change the pictures? (slot.'s
    # Preemptive Frames must not; RetroArch's Run-Ahead shows the game N frames
    # ahead from the start.)
    result["idle_same_as_ra0"] = {
        f"{fe}-ra{n}": seqs[(fe, n)][0] == seqs[(fe, 0)][0] for fe, n in configs}
    result["shown_frames"] = {f"{fe}-ra{n}": len(seqs[(fe, n)][0]) for fe, n in configs}

    head = "event  frame  buttons      " + "  ".join(f"{c:>12}" for c in result["configs"])
    print(head)
    for r in result["events"]:
        cells = ["   no change" if r["lag"][c] is None else f"{r['lag'][c]:>12}" for c in result["configs"]]
        print(f"{r['event']:>5}  {r['frame']:>5}  {r['buttons']:<11}  " + "  ".join(cells))
    print("shown frames:", result["shown_frames"])
    print("no button, same pictures as Run-Ahead 0:", result["idle_same_as_ra0"])
    if "--json" in sys.argv:
        with open(sys.argv[sys.argv.index("--json") + 1], "w") as f:
            json.dump(result, f, indent=1)


if __name__ == "__main__":
    main()
