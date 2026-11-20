#!/usr/bin/env bash
# Converts the built-in presets' ez-tree images to KTX2, a file format for
# compressed GPU textures. Existing output files are overwritten.
#
# Writes two sets with the same layout under assets/textures/:
#   uastc/  Basis Universal UASTC: a compatible loader converts these images to
#           a format supported by the target GPU. For glTF, use KHR_texture_basisu;
#           for Bevy, enable `basis-universal`.
#   bc7/    BC7: usable directly by GPUs that support it, commonly on desktops.
#           Check support before choosing it for mobile devices or glTF viewers.
# Both sets use Zstandard (zstd) compression to reduce file size.
#
# Layout within each set:
#   bark/<Kind>/color.ktx2      surface colour, encoded as sRGB
#   bark/<Kind>/normal.ktx2     lighting detail, using OpenGL (+Y) normal maps
#   bark/<Kind>/roughness.ktx2  surface roughness, identical in R, G, and B
#   leaves/<kind>.ktx2          sRGB colour with separate transparency (straight alpha)
# Load colour images as sRGB so the renderer converts them for lighting. Load
# normal and roughness images as linear data, without colour conversion.
#
# Every file includes mipmaps: progressively smaller images down to 1x1,
# used to keep distant textures from shimmering.
#
# Requirements: ImageMagick 7 (`magick`), KTX-Software 4.4+ (`ktx`) and
# ctt-cli 0.6 (`cargo install ctt-cli --version 0.6.1 --locked`).
#
# Usage: tools/textures/convert.sh [EZ_TREE_TEXTURES_DIR]
# The default source is prior_art/ez-tree/src/app/public/textures, a clone of
# ez-tree at the commit named in port_plan.md.

set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
src=${1:-$repo/prior_art/ez-tree/src/app/public/textures}
out=$repo/assets/textures
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

# Convert the bark and leaf images used by the presets in crates/ze_tree/presets.
bark_kinds=(Bark001 Bark002 Bark003)
leaf_kinds=(ash aspen oak pine)

# Write both encodings from one prepared PNG. Arguments: input PNG, output path
# within each set, colour space (srgb|linear), channels (rgb|rgba), and whether
# to optimise UASTC for smaller compressed files (yes|no).
encode() {
    local png=$1 rel=$2 space=$3 channels=$4 rdo=$5

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
    mkdir -p "$(dirname "$out/uastc/$rel")"
    ktx create --format "$format" --assign-tf "$tf" \
        --encode uastc --uastc-quality 3 "${rdo_args[@]}" --zstd 19 \
        --generate-mipmap --mipmap-filter lanczos4 \
        "$png" "$out/uastc/$rel"

    # Encode BC7 with the same colour space, keeping transparency when present.
    mkdir -p "$(dirname "$out/bc7/$rel")"
    ctt "$png" -o "$out/bc7/$rel" --format bc7 --quality slow \
        --input-color-space "$space" --output-color-space "$space" \
        --mipmap --mipmap-filter lanczos3 --zstd=19
}

# Prepare matching 8-bit PNG inputs for both encoders. Copy greyscale roughness
# into all three colour channels, including green, where glTF and Bevy read it.
for kind in "${bark_kinds[@]}"; do
    dir=$src/bark/${kind}_1K-JPG
    base=$dir/${kind}_1K-JPG
    magick "$base"_Color.jpg -alpha off -type TrueColor "PNG24:$stage/$kind-color.png"
    magick "$base"_NormalGL.jpg -alpha off -type TrueColor "PNG24:$stage/$kind-normal.png"
    magick "$base"_Roughness.jpg -alpha off -type TrueColor "PNG24:$stage/$kind-roughness.png"

    encode "$stage/$kind-color.png" "bark/$kind/color.ktx2" srgb rgb yes
    encode "$stage/$kind-normal.png" "bark/$kind/normal.ktx2" linear rgb no
    encode "$stage/$kind-roughness.png" "bark/$kind/roughness.ktx2" linear rgb yes
done

for kind in "${leaf_kinds[@]}"; do
    magick "$src/leaves/$kind.png" -type TrueColorAlpha "PNG32:$stage/$kind-leaf.png"
    encode "$stage/$kind-leaf.png" "leaves/$kind.ktx2" srgb rgba yes
done

cp "$repo/tools/textures/LICENSE.md" "$out/LICENSE.md"
echo "Wrote $out"
