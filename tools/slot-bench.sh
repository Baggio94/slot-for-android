#!/bin/bash
# slot.'s shelf bench on every handheld plugged in (2.5.0-beta38).
#
#   bash tools/slot-bench.sh [report folder, ~/slot-beta by default]
#
# Each handheld: slot. comes to the front, scrolls its carousel, changes
# shelves and opens menus by itself for about 45 seconds (SlotBench.kt), then
# its report (frames, insertions, ejections, launches, from slot.'s own
# journal) goes to bench-<model>.txt. Nothing is chosen and no game goes in.
# Eject any game first: the bench doesn't run while one is on.
set -u
out="${1:-$HOME/slot-beta}"
mkdir -p "$out"
report_on_device=/sdcard/Android/data/fyi.slot.konkr/files/slot-bench.txt

transports=$(adb devices -l | awk 'NR > 1 && $2 == "device" {
    for (i = 1; i <= NF; i++) if ($i ~ /^transport_id:/) { sub(/^transport_id:/, "", $i); print $i } }')
if [ -z "$transports" ]; then
    echo "No handheld found: plug one in and allow USB debugging."
    exit 1
fi

for t in $transports; do
    model=$(adb -t "$t" shell getprop ro.product.model | tr -d '\r' | tr -c 'A-Za-z0-9\n' '_')
    # An Android emulator on the Mac, or a device without slot.: skipped.
    if [ "$(adb -t "$t" shell getprop ro.kernel.qemu | tr -d '\r')" = "1" ] ||
        ! adb -t "$t" shell pm path fyi.slot.konkr 2>/dev/null | grep -q package:; then
        echo "== $model: skipped (emulator, or slot. isn't installed)"
        continue
    fi
    echo "== $model: about a minute, don't touch it"
    # Room in Android's log (some handhelds keep only 256 KB of it).
    adb -t "$t" logcat -G 16M > /dev/null 2>&1
    adb -t "$t" logcat -c
    adb -t "$t" shell rm -f "$report_on_device"
    # A new value each time: slot. runs the bench once per value.
    adb -t "$t" shell setprop debug.slot.bench "$(date +%s)"
    # Away and back, so slot. resumes and reads the property.
    adb -t "$t" shell am start -a android.settings.SETTINGS > /dev/null 2>&1
    sleep 2
    adb -t "$t" shell am start -n fyi.slot.konkr/.MainActivity > /dev/null 2>&1
    result="timed out"
    for _ in $(seq 1 75); do
        sleep 2
        line=$( (adb -t "$t" shell cat "$report_on_device" 2>/dev/null; \
            adb -t "$t" logcat -d -s SlotKonkr:I 2>/dev/null) | grep -E "Slot bench: (done|stopped)" | tail -1)
        case "$line" in
            *done*) result="done"; break ;;
            *stopped*) result="stopped (a game was on, or slot. went away)"; break ;;
        esac
    done
    sleep 1
    adb -t "$t" shell setprop debug.slot.bench 0
    report="$out/bench-$model.txt"
    {
        echo "slot. bench on $model: $result"
        adb -t "$t" shell dumpsys package fyi.slot.konkr | grep -m1 versionName
        adb -t "$t" shell cat "$report_on_device" 2>/dev/null ||
            adb -t "$t" logcat -d -v time | grep -E "Slot bench|Shelf frames|Insert timeline|Eject timeline|Cart sound|Slot launch|Core init"
    } > "$report"
    echo "   $result: $report"
done
