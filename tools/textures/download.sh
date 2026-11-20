#!/usr/bin/env bash
# Download ready-made bark and leaf KTX2 textures to run the viewer or texture tests.
# Uses assets/textures/assets_manifest.json to fetch files from the public
# gs://ze_textures/tree folder. Source images and conversion tools are not needed.
#
# By default, downloads 512 and 1k bark textures and all leaf textures in both
# BC7 and UASTC formats. Add --sizes 512,1k,2k for larger bark textures. Leaves
# have one size per kind, so --sizes does not filter them. Every leaf kind has
# colour files; the Nobiax kinds also include normal maps for lighting detail.
#
# Files go into assets/textures/bark/ and assets/textures/leaves/ by default.
# Rerunning is safe: files whose SHA-256 checksums match the manifest are kept.
# Missing or changed files are downloaded and verified before replacement;
# a checksum mismatch stops the script.
#
# Requirements: curl, jq, and shasum (macOS) or sha256sum.
# Usage: tools/textures/download.sh [--sizes 512,1k,2k] [--formats bc7,uastc] [--dest DIR]
# --sizes selects bark sizes only. --formats selects formats for both bark and
# leaves. --dest receives bark/ and leaves/ subfolders; it defaults to the
# repository's assets/textures/.

set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
manifest=$repo/assets/textures/assets_manifest.json
dest=$repo/assets/textures
sizes=512,1k
formats=bc7,uastc
ua='Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36'

# Read the options.
while [[ $# -gt 0 ]]; do
    case $1 in
        --sizes) sizes=$2; shift 2 ;;
        --formats) formats=$2; shift 2 ;;
        --dest) dest=$2; shift 2 ;;
        *) echo "unknown option $1" >&2; exit 1 ;;
    esac
done

# Get a file's SHA-256 checksum.
sha256() {
    if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# List the chosen files as "path sha256" lines.
base_url=$(jq -er '.baseUrl' "$manifest")
files=$(jq -r --arg sizes "$sizes" --arg formats "$formats" '
    ($sizes | split(",")) as $s | ($formats | split(",")) as $f
    | .assets[].files[]
    | select((.size == null or (.size | IN($s[]))) and (.format | IN($f[])))
    | "\(.path) \(.sha256)"' "$manifest")
if [[ -z $files ]]; then
    echo "no files match --sizes $sizes --formats $formats" >&2
    exit 1
fi

# Download each missing or changed file to a temporary name, check it, then move it into place.
kept=0 downloaded=0
while read -r path sum; do
    file=$dest/$path
    if [[ -f $file && $(sha256 "$file") == "$sum" ]]; then
        kept=$((kept + 1))
        continue
    fi
    mkdir -p "$(dirname "$file")"
    curl -fsS -A "$ua" -o "$file.part" "$base_url$path"
    if [[ $(sha256 "$file.part") != "$sum" ]]; then
        rm -f "$file.part"
        echo "checksum mismatch for $base_url$path" >&2
        exit 1
    fi
    mv "$file.part" "$file"
    downloaded=$((downloaded + 1))
done <<<"$files"
echo "Downloaded $downloaded files and kept $kept up-to-date files in $dest"
