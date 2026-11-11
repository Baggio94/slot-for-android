#!/usr/bin/env python3
"""One table for every game run by run.sh, plus the core options slot. leaves
to each core's internal fallback (the frontend sends nothing) where RetroArch
sends the declared default.

usage: summary.py [RESULTS_DIR] > summary.md
"""
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".build/bench/results")


def cell(report):
    if report is None:
        return "-"
    if report["verdict"] == "identical":
        return "identique"
    d = report["frames"]["differing_frames"]
    parts = [f"{k} {v}" for k, v in d.items() if v and k != "input"]
    end = report["end_match"]
    if not end["sram"]:
        parts.append("save")
    if report["frames"]["slot"] != report["frames"]["retroarch"]:
        parts.append("frames")
    return "différent (" + ", ".join(parts) + ")"


rows, unset = [], {}
for core_dir in sorted(p for p in root.iterdir() if p.is_dir()):
    for game in sorted(p for p in core_dir.iterdir() if p.is_dir()):
        reports = {}
        for mode in ("defaults", "frontend", "rewind", "options"):
            f = game / f"compare-{mode}.json"
            reports[mode] = json.loads(f.read_text()) if f.exists() else None
        rows.append((core_dir.name, game.name, reports))
        fix = reports["options"]
        if fix:
            for o in fix["options"]:
                if o["slot"] is None:
                    unset.setdefault(core_dir.name, {})[o["key"]] = o["retroarch"]

print("| Core | Jeu | slot. vs RetroArch (tels quels) | Frontend seul (mêmes options, sans Rewind) | Effet du Rewind de slot. | Effet « répondre à toutes les options » |")
print("|---|---|---|---|---|---|")
for core, game, r in rows:
    print(f"| {core} | {game} | {cell(r['defaults'])} | {cell(r['frontend'])} | {cell(r['rewind'])} | {cell(r['options'])} |")
print()
print("Options que slot. ne donne pas au core (le core prend sa valeur interne ; RetroArch envoie la valeur par défaut déclarée) :")
for core, opts in unset.items():
    print(f"\n**{core}** ({len(opts)})")
    for k, v in sorted(opts.items()):
        print(f"- `{k}` = `{v}`")
