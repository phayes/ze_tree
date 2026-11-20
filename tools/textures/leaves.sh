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
            # Build a colour background from coarse to fine copies of the image,
            # then overlay every pixel with any opacity at full strength, so those
            # keep their original colour, and restore the original transparency.
            local size s
            size=$(magick identify -format '%wx%h!' "$src/diffuse.tga")
            local args=("$src/diffuse.tga" -alpha set -write mpr:leaf +delete
                mpr:leaf -resize '1x1!' -alpha off -alpha set -resize "$size")
            for s in 2 4 8 16 32 64 128 256 512; do
                args+=('(' mpr:leaf -resize "${s}x${s}!" -resize "$size" ')'
                    -compose over -composite)
            done
            args+=('(' mpr:leaf '(' mpr:leaf -alpha extract -threshold 0 ')'
                    -compose copyalpha -composite ')' -compose over -composite
                '(' mpr:leaf -alpha extract ')' -compose copyalpha -composite
                -type TrueColorAlpha "PNG32:$out/$kind-color.png")
            magick "${args[@]}"
            magick "$src/normal.tga" -alpha off -type TrueColor "PNG24:$out/$kind-normal.png"
            ;;
        *)
            echo "unknown leaf source $source" >&2
            return 1
            ;;
    esac
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
