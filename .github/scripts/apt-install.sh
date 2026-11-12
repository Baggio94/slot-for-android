#!/usr/bin/env bash
# apt-get install that survives Ubuntu's archive being unreachable from
# GitHub's runners (2026-11-11: azure.archive.ubuntu.com, then
# archive.ubuntu.com, timed out on every package for hours). The runner's
# own mirrors are tried first, then other public Ubuntu mirrors, each with
# short timeouts so a dead one is left quickly.
set -uo pipefail
opts=(-o Acquire::http::Timeout=10 -o Acquire::https::Timeout=10 -o Acquire::Retries=1)
mirrors=(
  ""
  "http://mirrors.edge.kernel.org/ubuntu/"
  "http://mirror.math.princeton.edu/pub/ubuntu/"
  "http://ftp.uni-stuttgart.de/ubuntu/"
  "http://mirrors.mit.edu/ubuntu/"
)
for mirror in "${mirrors[@]}"; do
  if [ -n "$mirror" ]; then
    echo "apt-get failed: trying $mirror"
    printf '%s\n' "$mirror" | sudo tee /etc/apt/apt-mirrors.txt >/dev/null
    for file in /etc/apt/sources.list /etc/apt/sources.list.d/*; do
      [ -f "$file" ] && sudo sed -E -i \
        "s#https?://([a-z]+\.)?archive\.ubuntu\.com/ubuntu/?#${mirror}#g; s#https?://security\.ubuntu\.com/ubuntu/?#${mirror}#g" "$file"
    done
  fi
  if sudo apt-get "${opts[@]}" update -qq && sudo apt-get "${opts[@]}" install -y "$@"; then
    exit 0
  fi
done
exit 1
