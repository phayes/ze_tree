#!/usr/bin/env bash
# Download free-to-use bark textures from Poly Haven (https://polyhaven.com) and
# ambientCG (https://ambientcg.com). These images are released under CC0 and can
# be used in personal or commercial projects without attribution. Choose
# textures in tools/textures/sets.txt.
#
# Each texture includes three 2k images: colour, surface lighting detail (a
# normal map), and roughness, which controls how shiny the bark looks. Most are
# 2048 pixels square; some are twice as tall as they are wide. Normal maps use
# the OpenGL (+Y) convention; choose that setting in your renderer.
# Files are saved as <DEST>/<id>/{color,normal,roughness}.jpg for convert.sh,
# with <DEST>/<id>/info.json holding the title, authors, source and licence
# for manifest.sh. info.json is refreshed on every run.
#
# You can rerun this script. Poly Haven files that match the originals are kept,
# and missing or changed files are downloaded again. An ambientCG texture is
# downloaded only when one of its three images is missing. Each download is
# checked for damage; if the check fails, the damaged file is removed and the
# script stops.
#
# Install curl, jq, unzip, and either md5 (macOS) or md5sum before running.
# Usage: tools/textures/fetch.sh [DEST]
# DEST defaults to prior_art/textures in the repo, which Git ignores.

set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
dest=${1:-$repo/prior_art/textures}
ua='Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36'
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

# Get a file fingerprint (MD5 checksum) to check it against the original download.
checksum() {
    if command -v md5sum >/dev/null; then md5sum "$1" | cut -d' ' -f1; else md5 -q "$1"; fi
}

# Download a Poly Haven texture's 2k colour, OpenGL normal, and roughness maps,
# checking each against Poly Haven's published checksum.
fetch_polyhaven() {
    local id=$1

    # Save the texture's title and authors for manifest.sh.
    curl -fsS -A "$ua" "https://api.polyhaven.com/info/$id" |
        jq -e --arg id "$id" '{
            title: .name,
            authors: (.authors | keys),
            source: "polyhaven",
            sourceUrl: "https://polyhaven.com/a/\($id)",
            license: "CC0-1.0"
        }' >"$dest/$id/info.json"

    local listing
    listing=$(curl -fsS -A "$ua" "https://api.polyhaven.com/files/$id")

    for pair in Diffuse:color nor_gl:normal Rough:roughness; do
        local map=${pair%%:*} name=${pair#*:}
        local url md5 file=$dest/$id/$name.jpg
        url=$(jq -er --arg m "$map" '.[$m]["2k"].jpg.url' <<<"$listing")
        md5=$(jq -er --arg m "$map" '.[$m]["2k"].jpg.md5' <<<"$listing")

        # Keep a matching file; otherwise download it and check for damage.
        if [[ -f $file && $(checksum "$file") == "$md5" ]]; then
            continue
        fi
        curl -fsS -A "$ua" -o "$file" "$url"
        if [[ $(checksum "$file") != "$md5" ]]; then
            rm -f "$file"
            echo "checksum mismatch for $url" >&2
            exit 1
        fi
        echo "downloaded $id/$name.jpg"
    done
}

# Download an ambientCG texture's 2k JPEG archive and keep its colour, OpenGL
# normal, and roughness maps. ambientCG publishes no checksums, so the archive
# is checked against its published size and for internal damage instead.
fetch_ambientcg() {
    local id=$1
    local asset
    asset=$(curl -fsS -A "$ua" \
        "https://ambientcg.com/api/v2/full_json?id=$id&include=downloadData,displayData" |
        jq -ec '.foundAssets[0]')

    # Save the texture's title for manifest.sh. ambientCG's API names no
    # authors, so the author list is empty.
    jq -e --arg id "$id" '{
        title: .displayName,
        authors: [],
        source: "ambientcg",
        sourceUrl: "https://ambientcg.com/view?id=\($id)",
        license: "CC0-1.0"
    }' <<<"$asset" >"$dest/$id/info.json"

    if [[ -f $dest/$id/color.jpg && -f $dest/$id/normal.jpg && -f $dest/$id/roughness.jpg ]]; then
        return
    fi

    # Find the 2k JPEG archive and its expected size.
    local entry url size zip=$stage/$id.zip
    entry=$(jq -ec '.downloadFolders.default.downloadFiletypeCategories.zip.downloads[]
        | select(.attribute == "2K-JPG")' <<<"$asset")
    url=$(jq -er '.downloadLink' <<<"$entry")
    size=$(jq -er '.size' <<<"$entry")

    # Download and check the archive before extracting anything.
    curl -fsSL -A "$ua" -o "$zip" "$url"
    if [[ $(wc -c <"$zip" | tr -d ' ') != "$size" ]] || ! unzip -tq "$zip" >/dev/null; then
        echo "damaged download for $url" >&2
        exit 1
    fi

    # Extract only the three maps, without their folder paths, then rename them.
    local unpacked=$stage/$id
    mkdir -p "$unpacked"
    unzip -qjo "$zip" "${id}_2K-JPG_Color.jpg" "${id}_2K-JPG_NormalGL.jpg" \
        "${id}_2K-JPG_Roughness.jpg" -d "$unpacked"
    mv "$unpacked/${id}_2K-JPG_Color.jpg" "$dest/$id/color.jpg"
    mv "$unpacked/${id}_2K-JPG_NormalGL.jpg" "$dest/$id/normal.jpg"
    mv "$unpacked/${id}_2K-JPG_Roughness.jpg" "$dest/$id/roughness.jpg"
    echo "downloaded $id"
}

# Load the texture list; ignore comments and empty lines. Read it on its own
# file descriptor so the download tools can't consume it.
while read -r source id <&3; do
    [[ -z $source || $source == \#* ]] && continue
    mkdir -p "$dest/$id"
    case $source in
        polyhaven) fetch_polyhaven "$id" ;;
        ambientcg) fetch_ambientcg "$id" ;;
        *) echo "unknown source '$source' for $id" >&2; exit 1 ;;
    esac
done 3<"$repo/tools/textures/sets.txt"
echo "Images are in $dest"
