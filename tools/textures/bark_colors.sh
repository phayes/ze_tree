#!/usr/bin/env bash
# Print Rust entries for the BARK_AVERAGE_COLORS table in
# crates/ze_tree/src/material.rs. After adding or changing bark textures, run
# this script and copy its output into the table; it does not edit the file.
#
# Each line is `("<id>", [r, g, b]),`, with linear RGB channels from 0 to 1.
# These average colours approximate distant bark without loading texture images.
# Bark IDs and their output order come from tools/textures/sets.txt.
#
# Requirements: ImageMagick 7 (magick).
# Usage: tools/textures/bark_colors.sh [BARK_DIR]
# BARK_DIR defaults to prior_art/textures. Run fetch.sh first, or supply a folder
# containing <id>/color.jpg for every bark ID.

set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
bark_src=${1:-$repo/prior_art/textures}

while read -r source id; do
    [[ -z $source || $source == \#* ]] && continue

    # Average pixel colours in linear light rather than sRGB for correct brightness.
    rgb=$(magick "$bark_src/$id/color.jpg" -colorspace RGB -scale '1x1!' \
        -format '%[fx:r] %[fx:g] %[fx:b]' info:)
    read -r r g b <<<"$rgb"
    printf '    ("%s", [%.4f, %.4f, %.4f]),\n' "$id" "$r" "$g" "$b"
done <"$repo/tools/textures/sets.txt"
