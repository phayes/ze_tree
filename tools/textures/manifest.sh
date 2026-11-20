#!/usr/bin/env bash
# Write assets/textures/assets_manifest.json for downloading prepared KTX2 textures.
# Run convert.sh first: all expected bark and leaf files must exist, or this script
# stops with an error. The existing manifest is overwritten. Publish it alongside
# the matching textures in gs://ze_textures/tree so download paths and checksums
# agree. Other projects share the bucket under their own folders, so publish only
# into tree/:
#   gcloud storage rsync --recursive assets/textures/bark gs://ze_textures/tree/bark
#   gcloud storage rsync --recursive assets/textures/leaves gs://ze_textures/tree/leaves
#   gcloud storage cp assets/textures/assets_manifest.json assets/textures/LICENSE.md \
#       gs://ze_textures/tree/
#
# Entries include file paths, maps, compression formats, dimensions, byte counts,
# and SHA-256 checksums for verifying downloads. Each texture also includes its
# title, authors, source and licence. Bark entries follow tools/textures/sets.txt;
# leaf entries follow tools/textures/leaves.txt. Every leaf has colour files;
# Nobiax leaves also have normal maps for lighting detail. Both BC7 and UASTC
# formats are listed. Only bark files have size variants.
#
# Requirements: jq, KTX-Software 4.4+ (ktx), and shasum (macOS) or sha256sum.
# Usage: tools/textures/manifest.sh [BARK_DIR]
# BARK_DIR defaults to prior_art/textures, where fetch.sh saves each bark
# texture's info.json. Leaf attribution comes from leaves.txt and leaves.sh.

set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
source "$repo/tools/textures/leaves.sh"
bark_src=${1:-$repo/prior_art/textures}
out=$repo/assets/textures
base_url='https://storage.googleapis.com/ze_textures/tree/'
sizes=(512 1k 2k)
maps=(color normal roughness)
formats=(bc7 uastc)

# Get a file's SHA-256 checksum.
sha256() {
    if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# Load the bark ids from the texture list; ignore comments and empty lines.
bark_ids=()
while read -r source id; do
    [[ -z $source || $source == \#* ]] || bark_ids+=("$id")
done <"$repo/tools/textures/sets.txt"

# Describe one file as a JSON object. Arguments: path inside $out, then extra
# fields as a JSON object. Stops with an error if the file is missing.
describe_file() {
    local rel=$1 extra=$2 file=$out/$1
    if [[ ! -f $file ]]; then
        echo "missing $file" >&2
        exit 1
    fi

    # Read the real dimensions from the KTX2 header.
    local header
    header=$(ktx info --format json "$file" | jq -c '.header')
    jq -nc --arg path "$rel" --argjson extra "$extra" --argjson header "$header" \
        --argjson bytes "$(wc -c <"$file" | tr -d ' ')" --arg sha256 "$(sha256 "$file")" \
        '{path: $path} + $extra + {width: $header.pixelWidth, height: $header.pixelHeight,
          bytes: $bytes, sha256: $sha256}'
}

# Describe every file of one bark texture as a JSON object per line.
describe_bark_files() {
    local id=$1
    for size in "${sizes[@]}"; do
        for map in "${maps[@]}"; do
            for format in "${formats[@]}"; do
                describe_file "bark/$id-$size-$map-$format.ktx2" \
                    "$(jq -nc --arg size "$size" --arg map "$map" --arg format "$format" \
                        '{size: $size, map: $map, format: $format}')"
            done
        done
    done
}

# Combine each bark texture's details from fetch.sh with its file list.
bark_assets=$(for id in "${bark_ids[@]}"; do
    describe_bark_files "$id" | jq -s --arg id "$id" --slurpfile info "$bark_src/$id/info.json" \
        '{id: $id, category: "bark"} + $info[0] + {files: .}'
done | jq -s '.')

# List each leaf kind's colour and available normal maps with attribution and licence details.
leaf_assets=$(while IFS=$'\t' read -r kind source _ title; do
    info=$(leaf_info "$kind" "$source" "$title")
    for map in $(leaf_maps "$source"); do
        for format in "${formats[@]}"; do
            describe_file "leaves/$kind-$map-$format.ktx2" \
                "$(jq -nc --arg map "$map" --arg format "$format" '{map: $map, format: $format}')"
        done
    done | jq -s --arg id "$kind" --argjson info "$info" \
        '{id: $id, category: "leaves"} + $info + {files: .}'
done < <(leaf_list "$repo") | jq -s '.')

# Write the manifest with a short description of the maps and formats.
jq -n --arg base_url "$base_url" --argjson bark "$bark_assets" --argjson leaves "$leaf_assets" '{
    version: 1,
    baseUrl: $base_url,
    pathPatterns: {
        bark: "bark/<id>-<size>-<map>-<format>.ktx2",
        leaves: "leaves/<id>-<map>-<format>.ktx2"
    },
    sizes: {
        "512": "fits within 512 x 512 pixels",
        "1k": "fits within 1024 x 1024 pixels",
        "2k": "fits within 2048 x 2048 pixels"
    },
    maps: {
        color: "surface colour, with transparency for leaves; load as sRGB",
        normal: "surface lighting detail, OpenGL (+Y) convention; load as linear data",
        roughness: "how matte the surface is, identical in R, G and B; load as linear data"
    },
    formats: {
        bc7: "BC7 with Zstandard compression, for GPUs that support BC7, commonly desktops",
        uastc: "Basis Universal UASTC with Zstandard compression; the loader converts it to a format the GPU supports"
    },
    assets: ($bark + $leaves)
}' >"$out/assets_manifest.json"
echo "Wrote $out/assets_manifest.json"
