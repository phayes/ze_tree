# Leaf texture preparation and metadata helpers shared by convert.sh, images.sh
# and manifest.sh. Source this file from Bash; it defines functions without
# processing any files until a function is called.

# List leaf textures from tools/textures/leaves.txt in their original order.
# Output fields are tab-separated: kind, source, path, title. Blank lines and
# comments are skipped. Argument: repository root.
leaf_list() {
    local repo=$1 kind source path title
    while read -r kind source path title; do
        [[ -z $kind || $kind == \#* ]] && continue
        printf '%s\t%s\t%s\t%s\n' "$kind" "$source" "$path" "$title"
    done <"$repo/tools/textures/leaves.txt"
}

# Print the available map names for a source: "color" for ez-tree, or
# "color normal" for nobiax. Argument: source name; unknown names return an error.
leaf_maps() {
    case $1 in
        ez-tree) echo color ;;
        nobiax) echo color normal ;;
        *) echo "unknown leaf source $1" >&2; return 1 ;;
    esac
}

# Write <out>/<kind>-color.png and, for Nobiax textures, <kind>-normal.png.
# Outputs are 8-bit PNGs at the source dimensions. Colour uses sRGB with straight
# alpha (RGB is not multiplied by opacity). Normal maps describe lighting detail
# using OpenGL (+Y) directions, stored as linear data without transparency.
# Arguments: kind, source name, absolute source path, output folder.
#
# Fill colour behind Nobiax's transparent pixels to reduce dark outlines when
# textures are filtered or viewed from a distance. Restore the original alpha
# so the leaf silhouette stays unchanged.
prepare_leaf() {
    local kind=$1 source=$2 src=$3 out=$4
    mkdir -p "$out"
    case $source in
        ez-tree)
            magick "$src" -type TrueColorAlpha "PNG32:$out/$kind-color.png"
            ;;
        nobiax)
            bleed_color "$src/diffuse.tga" "$out/$kind-color.png"
            recolor_leaf "$kind" "$out/$kind-color.png"
            magick "$src/normal.tga" -alpha off -type TrueColor "PNG24:$out/$kind-normal.png"
            ;;
        *)
            echo "unknown leaf source $source" >&2
            return 1
            ;;
    esac
}

# Recolour a prepared leaf colour PNG in place for kinds that reuse another
# texture with new colours; other kinds are left unchanged. Transparency is kept.
# Arguments: leaf kind, colour PNG path.
#
# Kinds recoloured here:
# cedar: Nobiax's bronze autumn arborvitae in a bright cedar green.
# maple: Nobiax's autumn field maple in a bright summer green.
# Both are light enough for presets to darken or shift with a tint.
recolor_leaf() {
    local kind=$1 png=$2
    case $kind in
        cedar) regrade_green "$png" '#0e1f08' '#447224' '#b4d070' 70%,95% 160,40 ;;
        maple) regrade_green "$png" '#16330a' '#4c8a26' '#c4e07a' 45%,70% 150,35 ;;
    esac
}

# Recolour a leaf colour PNG in place to green, keeping its transparency.
# Each pixel's brightness picks a colour from a three-colour ramp, darkest first.
# Greyish pixels, such as twigs and stems, keep a muted version of their own colour.
# Arguments: PNG path; dark, middle and light ramp colours; the saturation range
# (an ImageMagick -level range on inverted saturation) treated as twig, where
# higher values select only greyer pixels; and the twig colour's
# -modulate brightness,saturation percentages.
regrade_green() {
    local png=$1 dark=$2 mid=$3 light=$4 twig_level=$5 twig_modulate=$6
    magick "$png" -write mpr:leaf +delete \
        '(' mpr:leaf -alpha off -colorspace gray -level 0%,60% \
            '(' -size 1x128 gradient:"$dark-$mid" \
                -size 1x128 gradient:"$mid-$light" -append -rotate 90 ')' \
            -clut ')' \
        '(' mpr:leaf -alpha off -modulate "$twig_modulate" ')' \
        '(' mpr:leaf -alpha off -colorspace HSL -channel G -separate +channel \
            -negate -level "$twig_level" -blur 0x1 ')' \
        -composite \
        '(' mpr:leaf -alpha extract ')' -alpha off -compose copyalpha -composite \
        -type TrueColorAlpha "PNG32:$png"
}

# Copy a leaf image to an 8-bit PNG, filling transparent areas with nearby leaf
# colours to reduce dark edges when textures are filtered or viewed from afar.
# Preserves the original transparency and visible colours. The input should use
# sRGB colour; the output uses straight alpha (colour separate from transparency).
# Arguments: input image path, output PNG path.
bleed_color() {
    local src=$1 dst=$2

    # Layer increasingly detailed copies to fill the background with leaf colours.
    # Restore the original colours wherever alpha is nonzero, then restore alpha.
    local size s
    size=$(magick identify -format '%wx%h!' "$src")
    local args=("$src" -alpha set -write mpr:leaf +delete
        mpr:leaf -resize '1x1!' -alpha off -alpha set -resize "$size")
    for s in 2 4 8 16 32 64 128 256 512; do
        args+=('(' mpr:leaf -resize "${s}x${s}!" -resize "$size" ')'
            -compose over -composite)
    done
    args+=('(' mpr:leaf '(' mpr:leaf -alpha extract -threshold 0 ')'
            -compose copyalpha -composite ')' -compose over -composite
        '(' mpr:leaf -alpha extract ')' -compose copyalpha -composite
        -type TrueColorAlpha "PNG32:$dst")
    magick "${args[@]}"
}

# Create <out>/<kind>-clump.png, a 256 x 256 image showing several leaves on one
# flat card for distant trees. Run prepare_leaf first to supply
# <out>/<kind>-color.png. Leaves fan out from the bottom centre so the card
# attaches to a branch like a single leaf. Output uses sRGB with straight alpha.
# Arguments: leaf kind, folder containing the prepared colour image.
prepare_clump() {
    local kind=$1 out=$2 src=$2/$1-color.png
    local w h
    read -r w h < <(magick identify -format '%w %h\n' "$src")

    # Each entry is "scale angle x y". Scale and rotate the leaf around its
    # bottom centre, then place that point at x, y on a 1024 x 1024 canvas.
    # Angles are in degrees; later entries are drawn over earlier ones.
    local copies=(
        "0.50 -65 440 960" "0.50 65 584 960"
        "0.45 -15 400 620" "0.45 20 630 600" "0.40 0 512 520"
        "0.55 -35 470 1000" "0.55 35 554 1000" "0.60 0 512 1010"
    )
    local args=(-size 1024x1024 xc:none) copy scale angle x y
    for copy in "${copies[@]}"; do
        read -r scale angle x y <<<"$copy"
        args+=('(' "$src" -alpha set -virtual-pixel transparent
            -define distort:viewport=1024x1024+0+0
            -distort SRT "$((w / 2)),$((h - 1)) $scale $angle $x,$y" ')'
            -compose over -composite)
    done

    # Shrink the composed image to 256 x 256, then fill transparent areas with
    # nearby colours to reduce dark edges during texture filtering.
    magick "${args[@]}" -filter Lanczos -resize 256x256 "PNG32:$out/$kind-clump-raw.png"
    bleed_color "$out/$kind-clump-raw.png" "$out/$kind-clump.png"
    rm "$out/$kind-clump-raw.png"
}

# Print attribution and licence metadata for one leaf texture as JSON.
# Arguments: kind, source name, title. Unknown sources return an error.
leaf_info() {
    local kind=$1 source=$2 title=$3
    case $source in
        ez-tree)
            jq -nc --arg title "$title" '{
                title: $title,
                authors: ["Daniel Greenheck"],
                source: "ez-tree",
                sourceUrl: "https://github.com/dgreenheck/ez-tree",
                license: "MIT"
            }'
            ;;
        nobiax)
            jq -nc --arg title "$title" '{
                title: $title,
                authors: ["Nobiax (yughues)"],
                source: "nobiax",
                sourceUrl: "https://nobiax.deviantart.com",
                license: "CC0-1.0"
            }'
            ;;
        *)
            echo "unknown leaf source $source" >&2
            return 1
            ;;
    esac
}
