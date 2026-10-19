#!/usr/bin/env bash
# Builds the player's download into dist/ (docs/development.md, "Packaging"):
#   macOS: ReRBR.app (the launcher, the viewer and rbr-standalone, SDL3 inside), ad-hoc signed, in
#          ReRBR-<version>-macos.dmg;
#   Linux: ReRBR-<version>-<arch>.AppImage (SDL3 built from source and linked in: needs cmake).
# The build is `cargo build --profile play` in rust/ ($CARGO_TARGET_DIR is honoured). The version is the launcher's.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/rust/rbr-launcher/Cargo.toml" | head -n 1)
target=${CARGO_TARGET_DIR:-$root/rust/target}
dist=$root/dist
bins=(rbr-launcher viewer rbr-standalone)

build() {
    (cd "$root/rust" && cargo build --profile play -p rbr-launcher -p rbr-render -p rbr-standalone \
        --bin rbr-launcher --bin viewer --bin rbr-standalone "$@")
}

macos() {
    build
    local app=$dist/ReRBR.app
    local macos=$app/Contents/MacOS frameworks=$app/Contents/Frameworks resources=$app/Contents/Resources
    rm -rf "$app"
    mkdir -p "$macos" "$frameworks" "$resources"
    for bin in "${bins[@]}"; do
        cp "$target/play/$bin" "$macos/"
    done

    # SDL3 (force feedback) is the one library outside the system: its dylib goes inside the bundle.
    local sdl
    sdl=$(otool -L "$macos/viewer" | awk '/libSDL3/ { print $1; exit }')
    [[ -n $sdl ]] || { echo "package.sh: the viewer links no SDL3" >&2; exit 1; }
    cp "$sdl" "$frameworks/libSDL3.0.dylib"
    chmod u+w "$frameworks/libSDL3.0.dylib"
    install_name_tool -id @rpath/libSDL3.0.dylib "$frameworks/libSDL3.0.dylib"
    for bin in "${bins[@]}"; do
        if otool -L "$macos/$bin" | grep -q "$sdl"; then
            install_name_tool -change "$sdl" @executable_path/../Frameworks/libSDL3.0.dylib "$macos/$bin"
        fi
    done
    local license
    license=$(cd "$(dirname "$sdl")/.." && pwd)/LICENSE.txt
    [[ -f $license ]] && cp "$license" "$resources/SDL3-LICENSE.txt"
    # The dylib's own minimum macOS is the bundle's.
    local minos
    minos=$(vtool -show-build "$frameworks/libSDL3.0.dylib" | awk '/minos/ { print $2; exit }')

    local iconset=$dist/ReRBR.iconset
    rm -rf "$iconset"
    mkdir -p "$iconset"
    for size in 16 32 128 256 512; do
        sips -z $size $size "$root/packaging/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
        sips -z $((size * 2)) $((size * 2)) "$root/packaging/icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$resources/ReRBR.icns"
    rm -rf "$iconset"

    cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>ReRBR</string>
    <key>CFBundleDisplayName</key><string>ReRBR</string>
    <key>CFBundleIdentifier</key><string>org.rerbr.ReRBR</string>
    <key>CFBundleExecutable</key><string>rbr-launcher</string>
    <key>CFBundleIconFile</key><string>ReRBR</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>${minos:-11.0}</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.racing-games</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

    # Ad-hoc signed, inside out (no Developer ID: docs/playing.md says how to open it the first time).
    codesign --force --sign - "$frameworks/libSDL3.0.dylib"
    for bin in "${bins[@]}"; do
        codesign --force --sign - "$macos/$bin"
    done
    codesign --force --sign - "$app"
    codesign --verify --strict "$app"

    local stage=$dist/dmg
    rm -rf "$stage"
    mkdir -p "$stage"
    cp -R "$app" "$stage/"
    ln -s /Applications "$stage/Applications"
    hdiutil create -quiet -volname "ReRBR $version" -srcfolder "$stage" -ov -format UDZO \
        "$dist/ReRBR-$version-macos.dmg"
    rm -rf "$stage"
    echo "$dist/ReRBR-$version-macos.dmg"
}

# appimagetool and its runtime, pinned by hash.
appimagetool_version=1.9.1
runtime_version=20251108
pinned_sum() {
    case $1 in
        appimagetool-x86_64.AppImage) echo ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 ;;
        appimagetool-aarch64.AppImage) echo f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158 ;;
        runtime-x86_64) echo 2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d ;;
        runtime-aarch64) echo 00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444 ;;
        *) echo "package.sh: no AppImage tools for $1" >&2; exit 1 ;;
    esac
}

fetch() {
    local name=$1 url=$2 path=$target/appimage/$1 sum
    sum=$(pinned_sum "$name")
    if [[ ! -f $path ]] || ! echo "$sum  $path" | sha256sum -c --quiet - 2>/dev/null; then
        mkdir -p "$target/appimage"
        curl -sSfL -o "$path.part" "$url"
        echo "$sum  $path.part" | sha256sum -c --quiet -
        mv "$path.part" "$path"
        chmod +x "$path"
    fi
    echo "$path"
}

linux() {
    local arch
    arch=$(uname -m)
    build --features rbr-render/sdl-static
    local appdir=$dist/ReRBR.AppDir
    rm -rf "$appdir"
    mkdir -p "$appdir/usr/bin"
    for bin in "${bins[@]}"; do
        cp "$target/play/$bin" "$appdir/usr/bin/"
    done
    ln -s usr/bin/rbr-launcher "$appdir/AppRun"
    cp "$root/packaging/icon.png" "$appdir/rerbr.png"
    ln -s rerbr.png "$appdir/.DirIcon"
    cat >"$appdir/rerbr.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=ReRBR
Comment=Richard Burns Rally with NGP physics, reimplemented
Exec=rbr-launcher
Icon=rerbr
Categories=Game;Simulation;
Terminal=false
EOF
    local tool runtime
    tool=$(fetch "appimagetool-$arch.AppImage" \
        "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-$arch.AppImage")
    runtime=$(fetch "runtime-$arch" \
        "https://github.com/AppImage/type2-runtime/releases/download/$runtime_version/runtime-$arch")
    # Without FUSE (containers, CI) appimagetool runs extracted.
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$arch "$tool" --no-appstream --runtime-file "$runtime" "$appdir" \
        "$dist/ReRBR-$version-$arch.AppImage"
    rm -rf "$appdir"
    echo "$dist/ReRBR-$version-$arch.AppImage"
}

mkdir -p "$dist"
case $(uname -s) in
    Darwin) macos ;;
    Linux) linux ;;
    *) echo "package.sh: macOS or Linux only" >&2; exit 1 ;;
esac
