#!/usr/bin/env python3
"""Prints a RetroArch core options file holding the option values slot. gave
the core (the last value of each option in a bench proxy log).

usage: options_from_log.py SLOT_LOG.jsonl > options.cfg
"""
import json
import sys

values = {}
with open(sys.argv[1]) as f:
    for line in f:
        try:
            e = json.loads(line)
        except json.JSONDecodeError:
            continue
        if e.get("event") == "get_variable" and e.get("value") is not None:
            values[e["key"]] = e["value"]
for key in sorted(values):
    value = values[key].replace("\\", "\\\\").replace('"', '\\"')
    print(f'{key} = "{value}"')
