#!/bin/sh
# switch-use installer — downloads a prebuilt binary from GitHub Releases.
# No npm, no Node, no tokens required.
#
#   curl -fsSL https://raw.githubusercontent.com/leeguooooo/switch-use/main/install.sh | sh
#
# Env overrides:
#   SWITCH_USE_VERSION=v0.1.0   install a specific tag (default: latest release)
#   SWITCH_USE_INSTALL_DIR=...  install dir (default: ~/.local/bin)
set -eu

REPO="leeguooooo/switch-use"
INSTALL_DIR="${SWITCH_USE_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${SWITCH_USE_VERSION:-}"

err() { printf 'switch-use-install: %s\n' "$1" >&2; exit 1; }

# --- detect platform ---------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Darwin)
    case "$arch" in
      arm64|aarch64) target="aarch64-apple-darwin" ;;
      x86_64)        err "Intel Macs have no prebuilt binary — build from source: 'cargo install --git https://github.com/leeguooooo/switch-use' (needs Rust)" ;;
      *) err "unsupported macOS arch: $arch" ;;
    esac ;;
  Linux)
    case "$arch" in
      x86_64)        target="x86_64-unknown-linux-gnu" ;;
      aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
      *) err "unsupported Linux arch: $arch" ;;
    esac ;;
  *) err "unsupported OS: $os (macOS and Linux only; on Windows use WSL)" ;;
esac

# --- resolve download URL ----------------------------------------------------
asset="switch-use-${target}.tar.gz"
if [ -n "$VERSION" ]; then
  base="https://github.com/${REPO}/releases/download/${VERSION}"
else
  base="https://github.com/${REPO}/releases/latest/download"
fi
url="${base}/${asset}"

# --- download + verify + install ---------------------------------------------
command -v curl >/dev/null 2>&1 || err "curl is required"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

printf 'switch-use-install: downloading %s\n' "$url"
curl -fSL --retry 3 -o "$tmp/$asset" "$url" || err "download failed (does the release have ${asset}?)"

# Optional checksum verification when the .sha256 sidecar is present.
if curl -fsSL --retry 2 -o "$tmp/$asset.sha256" "${url}.sha256" 2>/dev/null; then
  expected="$(awk '{print $1}' "$tmp/$asset.sha256")"
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')"
  else
    actual=""
  fi
  if [ -n "$actual" ] && [ "$expected" != "$actual" ]; then
    err "checksum mismatch (expected $expected, got $actual)"
  fi
  [ -n "$actual" ] && printf 'switch-use-install: checksum ok\n'
fi

tar -xzf "$tmp/$asset" -C "$tmp"
[ -f "$tmp/switch-use" ] || err "archive did not contain a 'switch-use' binary"

mkdir -p "$INSTALL_DIR"
# Stage next to the target, then rename: $tmp is often on another filesystem,
# where a plain mv copies over the existing (possibly running) binary in place
# and leaves a truncated file if interrupted. rename(2) within one dir is atomic.
stage="$INSTALL_DIR/.switch-use.new.$$"
trap 'rm -rf "$tmp"; rm -f "$stage"' EXIT
cp "$tmp/switch-use" "$stage"
chmod +x "$stage"
mv -f "$stage" "$INSTALL_DIR/switch-use"

printf 'switch-use-install: installed to %s/switch-use\n' "$INSTALL_DIR"
"$INSTALL_DIR/switch-use" --version >/dev/null 2>&1 && \
  printf 'switch-use-install: version %s\n' "$("$INSTALL_DIR/switch-use" --version 2>/dev/null)" || true

# --- PATH hint ---------------------------------------------------------------
case ":$PATH:" in
  *":$INSTALL_DIR:"*) : ;;
  *) printf 'switch-use-install: NOTE — add %s to your PATH:\n  export PATH="%s:$PATH"\n' "$INSTALL_DIR" "$INSTALL_DIR" ;;
esac
