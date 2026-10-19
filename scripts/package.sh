#!/usr/bin/env bash
# Builds the player's download into dist/ (docs/development.md, "Packaging"):
#   macOS: ReRBR.app (the launcher, the viewer and rbr-standalone), ad-hoc signed, in ReRBR-<version>-macos.dmg;
#   Linux: ReRBR-<version>-<arch>.AppImage;
#   Windows (Git Bash, MSVC): ReRBR-<version>-windows-x86_64.zip, the three .exe files in a ReRBR folder.
# All link SDL3 in, built from source (needs cmake), so nothing else needs installing.
# The build is `cargo build --profile play` ($CARGO_TARGET_DIR is honoured). The version is the launcher's.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/crates/rbr-launcher/Cargo.toml" | head -n 1)
target=${CARGO_TARGET_DIR:-$root/target}
dist=$root/dist
bins=(rerbr viewer rbr-standalone)
# The oldest macOS the app runs on: rustc, and cmake for SDL3, build for it.
export MACOSX_DEPLOYMENT_TARGET=11.0

build() {
    (cd "$root" && cargo build --profile play -p rbr-launcher -p rbr-render -p rbr-standalone \
        --bin rerbr --bin viewer --bin rbr-standalone --features rbr-render/sdl-static)
}

macos() {
    # SDL3's checks for macOS versions above the deployment target call clang's runtime, which rustc does not link.
    export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=$(clang --print-runtime-dir)/libclang_rt.osx.a"
    build
    local app=$dist/ReRBR.app
    local macos=$app/Contents/MacOS resources=$app/Contents/Resources
    rm -rf "$app"
    mkdir -p "$macos" "$resources"
    for bin in "${bins[@]}"; do
        cp "$target/play/$bin" "$macos/"
        # Only the system's libraries.
        if otool -L "$macos/$bin" | tail -n +2 | grep -v -e '^\s*/usr/lib/' -e '^\s*/System/'; then
            echo "package.sh: $bin links a library outside the system" >&2
            exit 1
        fi
    done

    cp "$root/scripts/packaging/icon.icns" "$resources/ReRBR.icns"

    cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>ReRBR</string>
    <key>CFBundleDisplayName</key><string>ReRBR</string>
    <key>CFBundleIdentifier</key><string>org.rerbr.ReRBR</string>
    <key>CFBundleExecutable</key><string>rerbr</string>
    <key>CFBundleIconFile</key><string>ReRBR</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.racing-games</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

    # Ad-hoc signed, inside out (no Developer ID: the README's Install says how to open it the first time).
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
    build
    local appdir=$dist/ReRBR.AppDir
    rm -rf "$appdir"
    mkdir -p "$appdir/usr/bin"
    for bin in "${bins[@]}"; do
        cp "$target/play/$bin" "$appdir/usr/bin/"
    done
    ln -s usr/bin/rerbr "$appdir/AppRun"
    cp "$root/scripts/packaging/icon.png" "$appdir/rerbr.png"
    ln -s rerbr.png "$appdir/.DirIcon"
    cat >"$appdir/rerbr.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=ReRBR
Comment=Richard Burns Rally with NGP physics, reimplemented
Exec=rerbr
Icon=rerbr
Categories=Game;Simulation;
Terminal=false
StartupWMClass=rerbr
EOF
    local tool runtime
    tool=$(fetch "appimagetool-$arch.AppImage" \
        "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-$arch.AppImage")
    runtime=$(fetch "runtime-$arch" \
        "https://github.com/AppImage/type2-runtime/releases/download/$runtime_version/runtime-$arch")
    # Without FUSE (containers, CI) appimagetool runs extracted. A copy without the AppImage magic in its ELF header
    # (bytes 8-10), which emulated x86_64 (Docker on Apple silicon) refuses to execute.
    cp "$tool" "$tool.run"
    printf '\0\0\0' | dd of="$tool.run" bs=1 seek=8 conv=notrunc status=none
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$arch "$tool.run" --no-appstream --runtime-file "$runtime" "$appdir" \
        "$dist/ReRBR-$version-$arch.AppImage"
    rm -rf "$appdir"
    echo "$dist/ReRBR-$version-$arch.AppImage"
}

windows() {
    # The C runtime linked in too (SDL3's cmake build follows), so players need no Visual C++ redistributable.
    export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
    build
    local folder=$dist/ReRBR zip=ReRBR-$version-windows-x86_64.zip
    rm -rf "$folder" "$dist/$zip"
    mkdir -p "$folder"
    for bin in "${bins[@]}"; do
        cp "$target/play/$bin.exe" "$folder/"
    done
    (cd "$dist" && powershell -NoProfile -Command "Compress-Archive -Path ReRBR -DestinationPath $zip")
    rm -rf "$folder"
    echo "$dist/$zip"
}

mkdir -p "$dist"
case $(uname -s) in
    Darwin) macos ;;
    Linux) linux ;;
    MINGW* | MSYS*) windows ;;
    *) echo "package.sh: macOS, Linux or Windows (Git Bash) only" >&2; exit 1 ;;
esac
