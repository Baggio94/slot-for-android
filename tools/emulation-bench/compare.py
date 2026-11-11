#!/usr/bin/env python3
"""Compares two bench proxy logs (slot. and RetroArch) of the same game.

usage: compare.py SLOT.jsonl RETROARCH.jsonl [--json OUT.json]

Reports, in this order:
  - core options whose final value differs between the two frontends
  - environment calls one frontend answers and the other refuses
  - the first frame whose picture, audio or size differs, and how many differ
  - input polling pattern differences (when and how often the pad is read)
  - battery save and savestate at the end
  - core run time per frame (CPU cost of the emulation itself)
"""
import json
import statistics
import sys

ENV_NAMES = {
    1: "SET_ROTATION", 2: "GET_OVERSCAN", 3: "GET_CAN_DUPE", 6: "SET_MESSAGE",
    8: "SET_PERFORMANCE_LEVEL", 9: "GET_SYSTEM_DIRECTORY", 10: "SET_PIXEL_FORMAT",
    11: "SET_INPUT_DESCRIPTORS", 13: "SET_DISK_CONTROL_INTERFACE", 14: "SET_HW_RENDER",
    15: "GET_VARIABLE", 16: "SET_VARIABLES", 17: "GET_VARIABLE_UPDATE",
    18: "SET_SUPPORT_NO_GAME", 19: "GET_LIBRETRO_PATH", 21: "SET_FRAME_TIME_CALLBACK",
    22: "SET_AUDIO_CALLBACK", 23: "GET_RUMBLE_INTERFACE", 24: "GET_INPUT_DEVICE_CAPABILITIES",
    25: "GET_SENSOR_INTERFACE", 26: "GET_CAMERA_INTERFACE", 27: "GET_LOG_INTERFACE",
    28: "GET_PERF_INTERFACE", 29: "GET_LOCATION_INTERFACE", 30: "GET_CORE_ASSETS_DIRECTORY",
    31: "GET_SAVE_DIRECTORY", 32: "SET_SYSTEM_AV_INFO", 33: "SET_PROC_ADDRESS_CALLBACK",
    34: "SET_SUBSYSTEM_INFO", 35: "SET_CONTROLLER_INFO", 36: "SET_MEMORY_MAPS",
    37: "SET_GEOMETRY", 38: "GET_USERNAME", 39: "GET_LANGUAGE",
    40: "GET_CURRENT_SOFTWARE_FRAMEBUFFER", 41: "GET_HW_RENDER_INTERFACE",
    42: "SET_SUPPORT_ACHIEVEMENTS", 43: "SET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE",
    44: "SET_SERIALIZATION_QUIRKS", 45: "SET_HW_SHARED_CONTEXT", 46: "GET_VFS_INTERFACE",
    47: "GET_LED_INTERFACE", 48: "GET_AUDIO_VIDEO_ENABLE", 49: "GET_MIDI_INTERFACE",
    50: "GET_FASTFORWARDING", 51: "GET_TARGET_REFRESH_RATE", 52: "GET_INPUT_BITMASKS",
    52: "GET_INPUT_BITMASKS", 53: "GET_CORE_OPTIONS_VERSION", 54: "SET_CORE_OPTIONS",
    55: "SET_CORE_OPTIONS_INTL", 56: "SET_CORE_OPTIONS_DISPLAY",
    57: "GET_PREFERRED_HW_RENDER", 58: "GET_DISK_CONTROL_INTERFACE_VERSION",
    59: "SET_DISK_CONTROL_EXT_INTERFACE", 60: "GET_MESSAGE_INTERFACE_VERSION",
    61: "SET_MESSAGE_EXT", 62: "GET_INPUT_MAX_USERS", 63: "SET_AUDIO_BUFFER_STATUS_CALLBACK",
    64: "SET_MINIMUM_AUDIO_LATENCY", 65: "SET_FASTFORWARDING_OVERRIDE",
    66: "SET_CONTENT_INFO_OVERRIDE", 67: "GET_GAME_INFO_EXT", 68: "SET_CORE_OPTIONS_V2",
    69: "SET_CORE_OPTIONS_V2_INTL", 70: "SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK",
    71: "SET_VARIABLE", 72: "GET_THROTTLE_STATE", 73: "GET_SAVESTATE_CONTEXT",
    74: "GET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE_SUPPORT", 75: "GET_JIT_CAPABLE",
    76: "GET_MICROPHONE_INTERFACE", 78: "GET_DEVICE_POWER", 79: "SET_NETPACKET_INTERFACE",
    80: "GET_PLAYLIST_DIRECTORY", 81: "GET_FILE_BROWSER_START_DIRECTORY",
}


def load(path):
    out = {"options": {}, "option_reads": {}, "env": {}, "frames": [], "end": None,
           "av": None, "events": []}
    with open(path) as f:
        for line in f:
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                continue
            kind = e.get("event")
            if kind == "get_variable":
                out["options"][e["key"]] = e["value"]
                out["option_reads"].setdefault(e["key"], []).append((e["frame"], e["value"]))
            elif kind == "env":
                key = (e["cmd"], e["experimental"])
                out["env"].setdefault(key, e)
            elif kind == "frame":
                out["frames"].append(e)
            elif kind == "end":
                out["end"] = e
            elif kind == "av_info":
                out["av"] = e
            elif kind in ("reset", "unserialize", "port_device", "variable_update"):
                out["events"].append(e)
    return out


def env_name(key):
    cmd, experimental = key
    return ENV_NAMES.get(cmd, f"cmd {cmd}") + (" (experimental)" if experimental else "")


def compare(slot, ra):
    report = {}
    opts = sorted(set(slot["options"]) | set(ra["options"]))
    report["options"] = [
        {"key": k, "slot": slot["options"].get(k), "retroarch": ra["options"].get(k)}
        for k in opts if slot["options"].get(k) != ra["options"].get(k)
    ]
    envs = sorted(set(slot["env"]) | set(ra["env"]))
    report["environment"] = []
    for key in envs:
        s, r = slot["env"].get(key), ra["env"].get(key)
        s_ok = s["ok"] if s else None
        r_ok = r["ok"] if r else None
        detail_s = {k: v for k, v in (s or {}).items() if k in ("value", "pixel_format")}
        detail_r = {k: v for k, v in (r or {}).items() if k in ("value", "pixel_format")}
        if s_ok != r_ok or detail_s != detail_r:
            report["environment"].append({"call": env_name(key), "slot": s_ok, "retroarch": r_ok,
                                          "slot_detail": detail_s, "retroarch_detail": detail_r})
    report["av_info"] = {"slot": slot["av"], "retroarch": ra["av"]}

    sf, rf = slot["frames"], ra["frames"]
    n = min(len(sf), len(rf))
    first = {"video": None, "audio": None, "size": None, "input": None}
    counts = {"video": 0, "audio": 0, "size": 0, "input": 0}
    for i in range(n):
        a, b = sf[i], rf[i]
        checks = {
            "video": a["video"] != b["video"],
            "audio": a["audio"] != b["audio"] or a["audio_frames"] != b["audio_frames"],
            "size": (a["w"], a["h"]) != (b["w"], b["h"]),
            "input": (a["polls"], a["reads"] > 0, a["order"]) != (b["polls"], b["reads"] > 0, b["order"]),
        }
        for k, differs in checks.items():
            if differs:
                counts[k] += 1
                if first[k] is None:
                    first[k] = {"frame": a["frame"], "slot": {x: a[x] for x in ("video", "audio", "audio_frames", "w", "h", "polls", "reads", "order")},
                                "retroarch": {x: b[x] for x in ("video", "audio", "audio_frames", "w", "h", "polls", "reads", "order")}}
    report["frames"] = {"slot": len(sf), "retroarch": len(rf), "compared": n,
                        "first_difference": first, "differing_frames": counts}

    def run_stats(frames):
        us = [f["run_us"] for f in frames[60:]] or [0]
        us_sorted = sorted(us)
        return {"median_us": statistics.median(us), "p99_us": us_sorted[int(len(us) * 0.99) - 1 if len(us) > 1 else 0],
                "max_us": us_sorted[-1]}
    report["core_time"] = {"slot": run_stats(sf), "retroarch": run_stats(rf),
                           "note": "slot. side is a debug test build; compare only large gaps"}
    report["end"] = {"slot": slot["end"], "retroarch": ra["end"]}
    se, re_ = slot["end"] or {}, ra["end"] or {}
    report["end_match"] = {"sram": se.get("sram") == re_.get("sram"),
                           "state": se.get("state") == re_.get("state")}
    report["events"] = {"slot": slot["events"][:20], "retroarch": ra["events"][:20]}
    same_emulation = (counts["video"] == 0 and counts["audio"] == 0 and counts["size"] == 0
                      and report["end_match"]["sram"] and len(sf) == len(rf))
    # The savestate is reported but not judged: some cores (bsnes, Gambatte,
    # gpSP) store host-side data in it, so two runs of the same frontend can
    # already differ there.
    report["verdict"] = "identical" if same_emulation else "different"
    return report


def print_report(r):
    print(f"Verdict: {r['verdict'].upper()}")
    fr = r["frames"]
    print(f"Frames: slot {fr['slot']}, RetroArch {fr['retroarch']}")
    for k, v in fr["differing_frames"].items():
        first = fr["first_difference"][k]
        where = f" (first at frame {first['frame']})" if first else ""
        print(f"  {k:6} differs on {v} frames{where}")
    if r["options"]:
        print("Core options that differ (slot. / RetroArch):")
        for o in r["options"]:
            print(f"  {o['key']}: {o['slot']!r} / {o['retroarch']!r}")
    if r["environment"]:
        print("Environment calls answered differently (slot. / RetroArch):")
        for e in r["environment"]:
            extra = ""
            if e["slot_detail"] or e["retroarch_detail"]:
                extra = f"  {e['slot_detail']} / {e['retroarch_detail']}"
            print(f"  {e['call']}: {e['slot']} / {e['retroarch']}{extra}")
    print(f"End: battery save {'same' if r['end_match']['sram'] else 'DIFFERENT'}, "
          f"savestate {'same' if r['end_match']['state'] else 'DIFFERENT'}")
    ct = r["core_time"]
    print(f"Core time per frame (median/p99 us): slot {ct['slot']['median_us']}/{ct['slot']['p99_us']}, "
          f"RetroArch {ct['retroarch']['median_us']}/{ct['retroarch']['p99_us']}")


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    r = compare(load(argv[1]), load(argv[2]))
    if "--json" in argv:
        with open(argv[argv.index("--json") + 1], "w") as f:
            json.dump(r, f, indent=1)
    print_report(r)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
