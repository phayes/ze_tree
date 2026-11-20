#!/usr/bin/env bash
# Prepare JPEG and PNG textures for models exported as glTF, GLB or OBJ.
# Writes to assets/textures/images/ and replaces existing output. Also writes
# images_manifest.json there, listing paths and download details for the public
# gs://ze_textures/tree/images folder. This script prepares files; it does not upload them.
#
# Bark: bark/<id>-<size>-<map>.jpg, relative to the output folder.
# Sizes 512, 1k and 2k fit within 512, 1024 and 2048 pixels square, preserving
# image proportions. Maps are color (sRGB), normal (OpenGL +Y lighting detail),
# and roughness (how matte the surface looks, equal in red, green and blue).
#
# Leaves: leaves/<kind>-<map>.png, using kinds from tools/textures/leaves.txt.
# Each kind keeps its source dimensions. Every kind has a color image with
# straight alpha: transparency is stored separately from RGB colour. Nobiax
# kinds also have a normal map, which adds lighting detail without extra geometry.
# Load colour as sRGB, and normal and roughness maps as linear data without colour
# conversion. Normal maps use the OpenGL (+Y) convention.
#
# The manifest records each file's path, map, dimensions, byte count and SHA-256
# checksum, plus texture attribution and licence details. Missing source images
# stop the script with an error.
#
# Requirements: ImageMagick 7 (magick), jq, and shasum (macOS) or sha256sum.
# Usage: tools/textures/images.sh [PRIOR_ART_DIR]
# PRIOR_ART_DIR defaults to prior_art/ in the repository. Run fetch.sh to populate
# its textures/ folder with bark sources. Set up ez-tree and Nobiax leaf sources
# as described in leaves.txt; their paths are relative to PRIOR_ART_DIR.
#
# To publish the generated files:
#   gcloud storage rsync --recursive assets/textures/images gs://ze_textures/tree/images

set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
source "$repo/tools/textures/leaves.sh"
prior_art=${1:-$repo/prior_art}
bark_src=$prior_art/textures
out=$repo/assets/textures/images
base_url='https://storage.googleapis.com/ze_textures/tree/images/'
sizes=(512:512 1k:1024 2k:2048)

# Get a file's SHA-256 checksum.
sha256() {
    if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# Describe one written image as a JSON object. Arguments: path inside $out,
# then extra fields as a JSON object.
describe() {
    local rel=$1 extra=$2 file=$out/$1
    local width height
    read -r width height < <(magick identify -format '%w %h\n' "$file" | head -1)
    jq -nc --arg path "$rel" --argjson extra "$extra" --argjson width "$width" \
        --argjson height "$height" --argjson bytes "$(wc -c <"$file" | tr -d ' ')" \
        --arg sha256 "$(sha256 "$file")" \
        '{path: $path} + $extra + {width: $width, height: $height, bytes: $bytes, sha256: $sha256}'
}

# Load the bark ids from the texture list; ignore comments and empty lines.
bark_ids=()
while read -r source id; do
    [[ -z $source || $source == \#* ]] || bark_ids+=("$id")
done <"$repo/tools/textures/sets.txt"

mkdir -p "$out/bark" "$out/leaves"

# Bark: fit each map within px × px as JPEG and describe each file.
bark_assets=$(for id in "${bark_ids[@]}"; do
    for map in color normal roughness; do
        if [[ ! -f $bark_src/$id/$map.jpg ]]; then
            echo "missing $bark_src/$id/$map.jpg" >&2
            exit 1
        fi
    done
    for size in "${sizes[@]}"; do
        name=${size%%:*} px=${size#*:}
        prefix=$out/bark/$id-$name

        # Resize colour in linear light to preserve average brightness. Keep
        # full colour resolution (4:4:4) so normal directions are not blurred.
        # Copy greyscale roughness into all three channels, including green,
        # where glTF reads it.
        magick "$bark_src/$id/color.jpg" -colorspace RGB -filter Lanczos -resize "${px}x${px}" \
            -colorspace sRGB -strip -quality 92 "$prefix-color.jpg"
        magick "$bark_src/$id/normal.jpg" -filter Lanczos -resize "${px}x${px}" \
            -strip -sampling-factor 4:4:4 -quality 95 "$prefix-normal.jpg"
        magick "$bark_src/$id/roughness.jpg" -filter Lanczos -resize "${px}x${px}" \
            -type TrueColor -strip -quality 92 "$prefix-roughness.jpg"

        for map in color normal roughness; do
            describe "bark/$id-$name-$map.jpg" \
                "$(jq -nc --arg size "$name" --arg map "$map" '{size: $size, map: $map}')"
        done
    done | jq -s --arg id "$id" --slurpfile info "$bark_src/$id/info.json" \
        '{id: $id, category: "bark"} + $info[0] + {files: .}'
done | jq -s '.')

# Write leaf colour and available normal maps as PNGs, recording each in the manifest.
leaf_assets=$(while IFS=$'\t' read -r kind source path title; do
    prepare_leaf "$kind" "$source" "$prior_art/$path" "$out/leaves"
    info=$(leaf_info "$kind" "$source" "$title")
    for map in $(leaf_maps "$source"); do
        describe "leaves/$kind-$map.png" "$(jq -nc --arg map "$map" '{map: $map}')"
    done | jq -s --arg id "$kind" --argjson info "$info" \
        '{id: $id, category: "leaves"} + $info + {files: .}'
done < <(leaf_list "$repo") | jq -s '.')

# Write the manifest with a short description of the files.
jq -n --arg base_url "$base_url" --argjson bark "$bark_assets" --argjson leaves "$leaf_assets" '{
    version: 1,
    baseUrl: $base_url,
    pathPatterns: {
        bark: "bark/<id>-<size>-<map>.jpg",
        leaves: "leaves/<id>-<map>.png"
    },
    sizes: {
        "512": "fits within 512 x 512 pixels",
        "1k": "fits within 1024 x 1024 pixels",
        "2k": "fits within 2048 x 2048 pixels"
    },
    maps: {
        color: "surface colour, sRGB",
        normal: "surface lighting detail, OpenGL (+Y) convention, linear data",
        roughness: "how matte the surface is, identical in R, G and B, linear data"
    },
    assets: ($bark + $leaves)
}' >"$out/images_manifest.json"
cp "$repo/assets/textures/LICENSE.md" "$out/LICENSE.md"
echo "Wrote $out"
