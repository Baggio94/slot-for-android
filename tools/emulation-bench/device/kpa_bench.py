#!/usr/bin/env python3
"""On-device half of the emulation bench: slot. vs RetroArch on a handheld
plugged into this computer over USB (adb). Runs on the computer (macOS or
Linux) with adb, ffmpeg and Python 3.

Subcommands (every result is a JSON file in --out):

  ra-backup                 copy RetroArch's retroarch.cfg to --out (do it first)
  ra-set KEY=VALUE...       change keys in RetroArch's retroarch.cfg (RetroArch
                            must be closed; it rewrites the file on exit)
  ra-restore                put the backed-up retroarch.cfg back
  pacing --app slot|ra      frame pacing of the running game: the exact present
                            time of every frame from SurfaceFlinger
  latency --app slot|ra     input lag on the lag test ROM (lagrom.py): presses
                            the real pad device, films the screen, times each
                            white->black switch
  audio --app slot|ra       the app's audio output: latency, buffer, underruns

The game must already be running in the app (slot. or RetroArch); nothing
here starts a game or touches saves.
"""
import argparse
import json
import os
import random
import re
import statistics
import subprocess
import sys
import time

PKG = {"slot": "fyi.slot.konkr", "ra": "com.retroarch.aarch64"}
RA_CFG = "/sdcard/Android/data/com.retroarch.aarch64/files/retroarch.cfg"
WORK = "/sdcard/slotbench"


def adb(*args, check=True, text=True, timeout=None):
    cmd = ["adb", "-d", *args]
    r = subprocess.run(cmd, capture_output=True, text=text, timeout=timeout)
    if check and r.returncode != 0:
        raise SystemExit(f"adb failed: {' '.join(args)}\n{r.stderr}")
    return r.stdout


def sh(command, **kw):
    return adb("shell", command, **kw)


def save(out, name, data):
    os.makedirs(out, exist_ok=True)
    path = os.path.join(out, name)
    with open(path, "w") as f:
        json.dump(data, f, indent=1)
    print(f"wrote {path}")
    return path


# ---------------------------------------------------------------- RetroArch
def ra_backup(a):
    os.makedirs(a.out, exist_ok=True)
    dest = os.path.join(a.out, "retroarch.cfg.backup")
    if os.path.exists(dest):
        raise SystemExit(f"{dest} exists already: keep the first backup")
    adb("pull", RA_CFG, dest)
    print(f"backed up to {dest}")


def ra_set(a):
    if sh(f"pidof {PKG['ra']}", check=False).strip():
        raise SystemExit("Close RetroArch first: it rewrites retroarch.cfg when it exits")
    local = os.path.join(a.out, "retroarch.cfg.work")
    adb("pull", RA_CFG, local)
    text = open(local).read()
    for pair in a.pairs:
        key, value = pair.split("=", 1)
        line = f'{key} = "{value}"'
        pattern = re.compile(rf'(?m)^{re.escape(key)}\s*=.*$')
        text, n = pattern.subn(line, text)
        if n == 0:
            text += line + "\n"
        print(line)
    open(local, "w").write(text)
    adb("push", local, RA_CFG)


def ra_restore(a):
    src = os.path.join(a.out, "retroarch.cfg.backup")
    if sh(f"pidof {PKG['ra']}", check=False).strip():
        raise SystemExit("Close RetroArch first")
    adb("push", src, RA_CFG)
    print("RetroArch settings restored")


# ------------------------------------------------------------------- pacing
def find_layer(app):
    names = sh("dumpsys SurfaceFlinger --list").splitlines()
    pkg = PKG[app]
    cands = [n.strip() for n in names
             if pkg in n and "SurfaceView" in n and "Background" not in n]
    if not cands:
        raise SystemExit(f"No {app} game layer on screen: is the game running?")
    # Android 12 BLAST: the buffer-holding child is the "(BLAST)" one.
    blast = [c for c in cands if "BLAST" in c]
    return (blast or cands)[-1]


def pacing(a):
    layer = find_layer(a.app)
    sh("dumpsys SurfaceFlinger --latency-clear", check=False)
    frames, period = {}, None
    end = time.time() + a.seconds
    while time.time() < end:
        out = sh(f"dumpsys SurfaceFlinger --latency '{layer}'").split()
        if out:
            period = int(out[0])
            vals = [int(v) for v in out[1:]]
            for i in range(0, len(vals) - 2, 3):
                desired, actual, ready = vals[i:i + 3]
                if 0 < actual < (1 << 62):
                    frames[actual] = (desired, ready)
        time.sleep(0.5)
    presents = sorted(frames)
    if len(presents) < 10 or not period:
        raise SystemExit("Too few frames measured")
    dts = [(b - a_) / 1e6 for a_, b in zip(presents, presents[1:])]
    per = period / 1e6
    vsyncs = [max(1, round(d / per)) for d in dts]
    late = [(p - frames[p][0]) / 1e6 for p in presents if frames[p][0] > 0]
    hist = {}
    for v in vsyncs:
        hist[v] = hist.get(v, 0) + 1
    span = (presents[-1] - presents[0]) / 1e9
    res = {
        "app": a.app, "label": a.label, "layer": layer, "seconds": span,
        "refresh_ms": per, "frames": len(presents),
        "fps": (len(presents) - 1) / span,
        "interval_ms": {"median": statistics.median(dts), "stdev": statistics.pstdev(dts),
                        "p99": sorted(dts)[int(len(dts) * 0.99)], "max": max(dts)},
        "vsyncs_per_frame": {str(k): v for k, v in sorted(hist.items())},
        "repeats": sum(v - 1 for v in vsyncs if v > 1),
        "gaps_over_25ms": sum(1 for d in dts if d > 25),
        "present_after_desired_ms": {"median": statistics.median(late) if late else None,
                                     "p99": sorted(late)[int(len(late) * 0.99)] if late else None},
    }
    save(a.out, f"pacing-{a.app}-{a.label}.json", res)
    print(json.dumps({k: res[k] for k in ("fps", "repeats", "gaps_over_25ms", "interval_ms")}, indent=1))


# ------------------------------------------------------------------ latency
def pad_device():
    out = sh("getevent -pl")
    dev = None
    for line in out.splitlines():
        m = re.match(r"add device \d+: (\S+)", line)
        if m:
            dev = m.group(1)
        elif "name:" in line and ("X-box" in line or "Xbox" in line or "pad" in line.lower()):
            return dev
    raise SystemExit("No gamepad input device found")


def latency(a):
    dev = pad_device()
    btn = a.button
    sh(f"mkdir -p {WORK}")
    gaps = [round(random.uniform(0.6, 1.0), 3) for _ in range(a.presses)]
    hold = 0.25
    total = int(sum(gaps) + a.presses * hold + 4)
    # Everything timed runs on the device: one clock, no USB jitter.
    script = f"""
rm -f {WORK}/lat.mp4 {WORK}/lat.txt
date +%s.%N > {WORK}/lat.txt.start
screenrecord --bit-rate 20000000 --time-limit {min(total, 180)} {WORK}/lat.mp4 &
REC=$!
sleep 2
for g in {' '.join(map(str, gaps))}; do
  echo "down $(date +%s.%N)" >> {WORK}/lat.txt
  sendevent {dev} 1 {btn} 1; sendevent {dev} 0 0 0
  sleep {hold}
  echo "up $(date +%s.%N)" >> {WORK}/lat.txt
  sendevent {dev} 1 {btn} 0; sendevent {dev} 0 0 0
  sleep $g
done
sleep 1
kill -INT $REC; wait $REC 2>/dev/null
"""
    sh(script, timeout=total + 60)
    os.makedirs(a.out, exist_ok=True)
    base = os.path.join(a.out, f"latency-{a.app}-{a.label}")
    adb("pull", f"{WORK}/lat.mp4", base + ".mp4")
    start = float(sh(f"cat {WORK}/lat.txt.start").strip())
    events = [(l.split()[0], float(l.split()[1]) - start)
              for l in sh(f"cat {WORK}/lat.txt").splitlines() if l.strip()]
    # Mean brightness of the screen's centre for every recorded frame.
    probe = subprocess.run(
        ["ffmpeg", "-hide_banner", "-i", base + ".mp4", "-vf",
         "crop=iw/3:ih/3:iw/3:ih/3,signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-",
         "-f", "null", "-"], capture_output=True, text=True)
    frames, t = [], None
    for line in probe.stdout.splitlines():
        m = re.search(r"pts_time:([\d.]+)", line)
        if m:
            t = float(m.group(1))
        m = re.search(r"YAVG=([\d.]+)", line)
        if m and t is not None:
            frames.append((t, float(m.group(1))))
    if not frames:
        raise SystemExit("No frames decoded from the recording (is ffmpeg installed?)")
    lo, hi = min(y for _, y in frames), max(y for _, y in frames)
    mid = (lo + hi) / 2
    dark = [(t, y < mid) for t, y in frames]
    switches = [(t, d) for (t0, d0), (t, d) in zip(dark, dark[1:]) if d != d0]
    lags = {"down": [], "up": []}
    for kind, when in events:
        want_dark = kind == "down"
        nxt = next((t for t, d in switches if d == want_dark and t > when - 0.05), None)
        if nxt is not None and nxt - when < 0.4:
            lags[kind].append((nxt - when) * 1000)
    res = {"app": a.app, "label": a.label, "presses": a.presses,
           "note": "times include a constant recording offset: compare apps, not absolute values",
           "contrast": [lo, hi], "switches": len(switches)}
    for kind in ("down", "up"):
        v = lags[kind]
        res[kind] = {"n": len(v), "median_ms": statistics.median(v) if v else None,
                     "mean_ms": statistics.mean(v) if v else None,
                     "stdev_ms": statistics.pstdev(v) if v else None,
                     "min_ms": min(v) if v else None, "max_ms": max(v) if v else None,
                     "all_ms": [round(x, 1) for x in v]}
    save(a.out, f"latency-{a.app}-{a.label}.json", res)
    print(json.dumps({k: res[k] for k in ("down", "up")}, indent=1)[:600])


# -------------------------------------------------------------------- audio
def audio(a):
    pid = sh(f"pidof {PKG[a.app]}", check=False).strip()
    if not pid:
        raise SystemExit(f"{a.app} is not running")
    def snap():
        return sh("dumpsys media.audio_flinger")
    first = snap()
    time.sleep(a.seconds)
    last = snap()
    os.makedirs(a.out, exist_ok=True)
    for name, text in (("before", first), ("after", last)):
        open(os.path.join(a.out, f"audio-{a.app}-{a.label}-{name}.txt"), "w").write(text)

    def tracks(text):
        # Track rows of the app's process: keep the header so the columns
        # (Underruns, Latency, FrmCnt, ...) can be read for each Android build.
        rows, header = [], None
        for line in text.splitlines():
            if re.search(r"\bType\b.*\bId\b.*\bUID\b", line) or "Underruns" in line and "Id" in line:
                header = line
            if re.match(rf"\s*\S*\s+{pid}\s", line) or f" {pid} " in line:
                rows.append(line)
        return header, rows
    h1, r1 = tracks(first)
    h2, r2 = tracks(last)
    outputs = re.findall(r"(?ms)^Output thread.*?(?=^Output thread|\Z)", last)
    res = {"app": a.app, "label": a.label, "pid": pid, "seconds": a.seconds,
           "track_header": h2, "tracks_before": r1, "tracks_after": r2,
           "output_threads": [o[:1500] for o in outputs if pid in o]}
    save(a.out, f"audio-{a.app}-{a.label}.json", res)
    print("\n".join([h2 or ""] + r2))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--out", default="kpa-bench-results")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("ra-backup")
    s = sub.add_parser("ra-set"); s.add_argument("pairs", nargs="+")
    sub.add_parser("ra-restore")
    for name in ("pacing", "latency", "audio"):
        s = sub.add_parser(name)
        s.add_argument("--app", choices=["slot", "ra"], required=True)
        s.add_argument("--label", default="run")
        if name in ("pacing", "audio"):
            s.add_argument("--seconds", type=float, default=60)
        if name == "latency":
            s.add_argument("--presses", type=int, default=40)
            s.add_argument("--button", type=int, default=304, help="evdev key code (304 = BTN_A)")
    a = p.parse_args()
    {"ra-backup": ra_backup, "ra-set": ra_set, "ra-restore": ra_restore,
     "pacing": pacing, "latency": latency, "audio": audio}[a.cmd](a)


if __name__ == "__main__":
    main()
