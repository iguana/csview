#!/usr/bin/env bash
#
# scripts/release.sh — build BOTH csview and csviewai release bundles and
# snapshot them into target/releases/ so they don't clobber each other.
#
# Why this is non-trivial:
#   1. Tauri 2 CLI ignores --config when picking which Rust crate to compile;
#      it just walks up cwd looking for a directory literally named src-tauri/.
#      So building csviewai requires renaming src-tauri-ai -> src-tauri (and
#      pruning the workspace members list to avoid a duplicate-package error).
#   2. target/release/bundle/dmg/ is non-additive — each `tauri build` reinits
#      that dir and wipes any sibling .dmg from a previous build. We snapshot
#      after each build so both end up in target/releases/.
#
# Usage:  scripts/release.sh
# Output: target/releases/{csview,csviewai}.app + *_0.2.0_aarch64.dmg
#
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
RELEASES="$ROOT/target/releases"

# --- preflight: bail on stale rename-swap state from a prior aborted run ---
for stale in "$ROOT/_src-tauri-base" "$ROOT/Cargo.toml.bak"; do
  if [ -e "$stale" ]; then
    echo "ABORT: stale $stale exists from a prior run; resolve before retrying" >&2
    exit 1
  fi
done
if [ -L "$ROOT/src-tauri" ]; then
  echo "ABORT: src-tauri is a symlink; resolve before retrying" >&2
  exit 1
fi

# --- clean ---
echo "[release] cleaning bundles, frontends, leaf-crate targets"
rm -rf "$ROOT/target/release/bundle" "$ROOT/dist" "$ROOT/dist-ai" \
       "$ROOT"/*.tsbuildinfo \
       "$ROOT/target/release/csview" "$ROOT/target/release/csviewai"
cargo clean -p csview -p csviewai >/dev/null 2>&1 || true

mkdir -p "$RELEASES"
rm -rf "$RELEASES"/csview.app "$RELEASES"/csviewai.app \
       "$RELEASES"/csview_*.dmg "$RELEASES"/csviewai_*.dmg

# --- build base csview ---
echo
echo "[release] === building csview ==="
pnpm tauri build
cp -p   "$ROOT/target/release/bundle/dmg/csview_"*"_aarch64.dmg" "$RELEASES/"
cp -RpP "$ROOT/target/release/bundle/macos/csview.app"           "$RELEASES/"
echo "[release] csview snapshotted -> $RELEASES"

# --- build csviewai via rename swap ---
restore_layout() {
  echo "[release] restoring src-tauri layout"
  if [ -d "$ROOT/_src-tauri-base" ]; then
    if [ -d "$ROOT/src-tauri" ] && [ ! -d "$ROOT/src-tauri-ai" ]; then
      mv "$ROOT/src-tauri" "$ROOT/src-tauri-ai"
    fi
    mv "$ROOT/_src-tauri-base" "$ROOT/src-tauri"
  fi
  if [ -f "$ROOT/Cargo.toml.bak" ]; then
    mv "$ROOT/Cargo.toml.bak" "$ROOT/Cargo.toml"
  fi
}
trap restore_layout EXIT

cp "$ROOT/Cargo.toml" "$ROOT/Cargo.toml.bak"
mv "$ROOT/src-tauri" "$ROOT/_src-tauri-base"
mv "$ROOT/src-tauri-ai" "$ROOT/src-tauri"
cat > "$ROOT/Cargo.toml" <<TOML
[workspace]
members = ["csview-engine", "src-tauri"]
resolver = "2"
TOML

echo
echo "[release] === building csviewai (rename-swap) ==="
pnpm tauri build
cp -p   "$ROOT/target/release/bundle/dmg/csviewai_"*"_aarch64.dmg" "$RELEASES/"
cp -RpP "$ROOT/target/release/bundle/macos/csviewai.app"           "$RELEASES/"
echo "[release] csviewai snapshotted -> $RELEASES"

# Run restore now so verification happens against the real layout, then disarm
# the trap so we don't double-restore.
trap - EXIT
restore_layout
# Cargo.lock can drift while the workspace was pruned — restore it from git.
git checkout -- "$ROOT/Cargo.lock" 2>/dev/null || true

# --- verify ---
echo
echo "[release] verifying $RELEASES"
fail=0
for app in "$RELEASES"/csview.app "$RELEASES"/csviewai.app; do
  name=$(basename "$app" .app)
  bin="$app/Contents/MacOS/$name"
  if [ ! -x "$bin" ]; then
    echo "  FAIL: $bin missing or not executable" >&2
    fail=1; continue
  fi
  cfname=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleName'       "$app/Contents/Info.plist")
  cfid=$(  /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist")
  size=$(stat -f '%z' "$bin")
  arch=$(file -b "$bin" | sed 's/.*executable //')
  if [ "$cfname" != "$name" ]; then
    echo "  FAIL: $app CFBundleName=$cfname (expected $name)" >&2
    fail=1; continue
  fi
  printf "  ok %-12s  bin=%s  %d bytes  %s  id=%s\n" "$name.app" "$name" "$size" "$arch" "$cfid"
done

for dmg in "$RELEASES"/csview_*_aarch64.dmg "$RELEASES"/csviewai_*_aarch64.dmg; do
  if [ ! -f "$dmg" ]; then
    echo "  FAIL: missing $dmg" >&2
    fail=1; continue
  fi
  size=$(stat -f '%z' "$dmg")
  printf "  ok %-40s  %d bytes\n" "$(basename "$dmg")" "$size"
done

if [ "$fail" != 0 ]; then
  echo "[release] verification failed" >&2
  exit 1
fi

echo
echo "[release] done."
