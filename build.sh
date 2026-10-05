#!/usr/bin/env bash
# builds everything, needs podman or docker
# ./build.sh [--version X.Y.Z] [--install]
set -euo pipefail

cd "$(dirname "$0")"

ID="st.lynx.plugins.opendeck-ampgd6.sdPlugin"
IMAGE="ghcr.io/rust-cross/cargo-zigbuild:sha-eba2d7e"
PLUGIN_DIR="$HOME/.config/opendeck/plugins/$ID"

version=""
install=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) version="$2"; shift 2 ;;
        --install) install=true; shift ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

if [[ -n "$version" ]]; then
    echo "==> Setting version to $version"
    sed -i "s/\"Version\": \".*\"/\"Version\": \"$version\"/" manifest.json
    sed -i "s/^version = \".*\"$/version = \"$version\"/" Cargo.toml
fi

if command -v podman >/dev/null; then
    engine=podman
elif command -v docker >/dev/null; then
    engine=docker
else
    echo "Need podman or docker installed" >&2
    exit 1
fi

run() {
    "$engine" run --rm -v "$(pwd)":/io:Z -w /io "$IMAGE" sh -c "$1"
}

echo "==> Building Linux"
run "cargo zigbuild --release --target x86_64-unknown-linux-gnu --target-dir target/plugin-linux"

echo "==> Building Mac"
run "cargo zigbuild --release --target universal2-apple-darwin --target-dir target/plugin-mac"

echo "==> Building Windows"
run "apt-get update -qq && apt-get install -y -qq mingw-w64 > /dev/null 2>&1 && cargo zigbuild --release --target x86_64-pc-windows-gnu --target-dir target/plugin-win"

echo "==> Packaging"
rm -rf build
mkdir -p "build/$ID"
cp -r assets manifest.json "build/$ID"
cp target/plugin-linux/x86_64-unknown-linux-gnu/release/opendeck-ampgd6 "build/$ID/opendeck-ampgd6-linux"
cp target/plugin-mac/universal2-apple-darwin/release/opendeck-ampgd6 "build/$ID/opendeck-ampgd6-mac"
cp target/plugin-win/x86_64-pc-windows-gnu/release/opendeck-ampgd6.exe "build/$ID/opendeck-ampgd6-win.exe"
(cd build && python3 -c "import shutil; shutil.make_archive('opendeck-ampgd6.plugin', 'zip', '.', '$ID')")

if $install; then
    echo "==> Installing into $PLUGIN_DIR"
    mkdir -p "$PLUGIN_DIR"
    cp -r "build/$ID/assets" "$PLUGIN_DIR/"
    # plain cp breaks if its running
    for file in manifest.json opendeck-ampgd6-linux opendeck-ampgd6-mac opendeck-ampgd6-win.exe; do
        cp "build/$ID/$file" "$PLUGIN_DIR/$file.new"
        mv -f "$PLUGIN_DIR/$file.new" "$PLUGIN_DIR/$file"
    done
    echo "Restart OpenDeck to load the new version"
fi

echo "==> Done: build/opendeck-ampgd6.plugin.zip"
