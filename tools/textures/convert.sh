#!/usr/bin/env bash
# Convert bark and leaf source images to KTX2 files for GPU rendering.
# Existing output files are overwritten. Run manifest.sh afterwards to update
# assets/textures/assets_manifest.json for downloads.
#
# Every map is written in two formats, both with Zstandard compression:
#   bc7    Read directly by GPUs that support BC7, including most desktop GPUs.
#   uastc  Basis Universal: a compatible loader converts it for the target GPU.
#          In Bevy, enable the basis-universal feature.
# Each file includes mipmaps, smaller copies down to 1x1 that reduce shimmering
# when textures are viewed from a distance.
#
# Bark: assets/textures/bark/<id>-<size>-<map>-<format>.ktx2
# Sizes 512, 1k and 2k fit within 512, 1024 and 2048 pixels square, preserving
# image proportions. Smaller sizes use less memory; larger sizes retain detail.
# Bark maps are color (sRGB), normal (OpenGL +Y lighting detail), and roughness
# (how matte the surface looks, with the same value in red, green and blue).
#
# Leaves: assets/textures/leaves/<kind>-<map>-<format>.ktx2
# Kinds and source paths come from tools/textures/leaves.txt. Each kind keeps
# its source dimensions, with no size variants. Every kind has a color map;
# Nobiax kinds also have a normal map for lighting detail without extra geometry.
# Every leaf kind also gets a 256 x 256 clump image showing several leaves on
# one flat card. Use these sRGB images with transparency for distant trees
# drawn with fewer, larger cards.
# Colour uses sRGB with straight alpha: transparency is separate from RGB colour.
# Load normal maps as linear data using the OpenGL (+Y) convention. Bark roughness
# maps also use linear data, without sRGB colour conversion.
#
# Requirements: ImageMagick 7 (magick), KTX-Software 4.4+ (ktx), and
# ctt-cli 0.6 (cargo install ctt-cli --version 0.6.1 --locked).
# Usage: tools/textures/convert.sh [--only bark|leaves|clumps] [--kind KIND] [PRIOR_ART_DIR]
# By default, convert all bark and leaf images, including clumps. Use --only leaves
# to skip bark, or --only clumps to regenerate clump textures without replacing
# regular leaf or bark textures. Use --kind to convert only one leaf kind from
# leaves.txt, such as a newly added one; bark is skipped. PRIOR_ART_DIR defaults
# to prior_art/ in the repository.
# Run fetch.sh for bark sources in its textures/ folder. For leaves, clone ez-tree
# at the commit in port_plan.md and extract Nobiax's packs as described in
# leaves.txt. All leaf source paths are relative to PRIOR_ART_DIR.

set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
source "$repo/tools/textures/leaves.sh"

# Read the options.
only=
leaf_kind=
while [[ $# -gt 0 && $1 == --* ]]; do
    case $1 in
        --only) only=$2; shift 2 ;;
        --kind) leaf_kind=$2; shift 2 ;;
        *) echo "unknown option $1" >&2; exit 1 ;;
    esac
done
if [[ -n $only && $only != bark && $only != leaves && $only != clumps ]]; then
    echo "--only takes bark, leaves or clumps" >&2
    exit 1
fi
prior_art=${1:-$repo/prior_art}
bark_src=$prior_art/textures
out=$repo/assets/textures
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

sizes=(512:512 1k:1024 2k:2048)

# Write both encodings from one prepared PNG. Arguments: input PNG, UASTC output
# path, BC7 output path, colour space (srgb|linear), channels (rgb|rgba), and
# whether to optimise UASTC for smaller compressed files (yes|no).
encode() {
    local png=$1 uastc=$2 bc7=$3 space=$4 channels=$5 rdo=$6

    # Tell the UASTC encoder which channels and colour encoding the PNG contains.
    local format tf
    if [[ $space == srgb ]]; then tf=srgb; else tf=linear; fi
    case "$channels-$space" in
        rgb-srgb) format=R8G8B8_SRGB ;;
        rgb-linear) format=R8G8B8_UNORM ;;
        rgba-srgb) format=R8G8B8A8_SRGB ;;
        *) echo "unsupported $channels-$space" >&2; exit 1 ;;
    esac
    local rdo_args=()
    if [[ $rdo == yes ]]; then rdo_args=(--uastc-rdo); fi
    mkdir -p "$(dirname "$uastc")"
    ktx create --format "$format" --assign-tf "$tf" \
        --encode uastc --uastc-quality 3 "${rdo_args[@]}" --zstd 19 \
        --generate-mipmap --mipmap-filter lanczos4 \
        "$png" "$uastc"

    # Encode BC7 with the same colour space, keeping transparency when present.
    mkdir -p "$(dirname "$bc7")"
    ctt "$png" -o "$bc7" --format bc7 --quality slow \
        --input-color-space "$space" --output-color-space "$space" \
        --mipmap --mipmap-filter lanczos3 --zstd=19
}

# Load the bark ids from the texture list; ignore comments and empty lines.
bark_ids=()
while read -r source id; do
    [[ -z $source || $source == \#* ]] || bark_ids+=("$id")
done <"$repo/tools/textures/sets.txt"

[[ $only == leaves || $only == clumps || -n $leaf_kind ]] && bark_ids=()
for id in "${bark_ids[@]}"; do
    for size in "${sizes[@]}"; do
        name=${size%%:*} px=${size#*:}
        base=$stage/$id-$name

        # Fit each image within px × px, keeping its shape, as 8-bit PNG inputs
        # for both encoders. Resize colour in linear light to preserve average
        # brightness. Copy greyscale roughness into all three colour channels,
        # including green, where glTF and Bevy read it.
        magick "$bark_src/$id/color.jpg" -colorspace RGB -filter Lanczos -resize "${px}x${px}" \
            -colorspace sRGB -alpha off -type TrueColor "PNG24:$base-color.png"
        magick "$bark_src/$id/normal.jpg" -filter Lanczos -resize "${px}x${px}" \
            -alpha off -type TrueColor "PNG24:$base-normal.png"
        magick "$bark_src/$id/roughness.jpg" -filter Lanczos -resize "${px}x${px}" \
            -alpha off -type TrueColor "PNG24:$base-roughness.png"

        prefix=$out/bark/$id-$name
        encode "$base-color.png" "$prefix-color-uastc.ktx2" "$prefix-color-bc7.ktx2" \
            srgb rgb yes
        encode "$base-normal.png" "$prefix-normal-uastc.ktx2" "$prefix-normal-bc7.ktx2" \
            linear rgb no
        encode "$base-roughness.png" "$prefix-roughness-uastc.ktx2" \
            "$prefix-roughness-bc7.ktx2" linear rgb yes
    done
done

# Prepare each leaf colour image and available normal map, then write both GPU formats.
if [[ $only != bark ]]; then
    while IFS=$'\t' read -r kind source path _; do
        [[ -n $leaf_kind && $kind != "$leaf_kind" ]] && continue
        prepare_leaf "$kind" "$source" "$prior_art/$path" "$stage/leaves"
        prepare_clump "$kind" "$stage/leaves"
        prefix=$out/leaves/$kind
        if [[ $only != clumps ]]; then
            encode "$stage/leaves/$kind-color.png" "$prefix-color-uastc.ktx2" \
                "$prefix-color-bc7.ktx2" srgb rgba yes
            if [[ -f $stage/leaves/$kind-normal.png ]]; then
                encode "$stage/leaves/$kind-normal.png" "$prefix-normal-uastc.ktx2" \
                    "$prefix-normal-bc7.ktx2" linear rgb no
            fi
        fi
        encode "$stage/leaves/$kind-clump.png" "$prefix-clump-uastc.ktx2" \
            "$prefix-clump-bc7.ktx2" srgb rgba yes
    done < <(leaf_list "$repo")
fi

cp "$repo/tools/textures/LICENSE.md" "$out/LICENSE.md"
echo "Wrote $out"
