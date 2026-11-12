#!/usr/bin/env bash
# Deploy the yellod suite web backend (suite-web) on this VM.
# Usage:
#   ./deploy.sh                # build (if needed) + install to ~/suite-web (no root needed)
#   sudo ./deploy.sh --system  # also install/enable systemd system service (needs root)
#
# The existing notes app (wrangler on :8787) is never touched.
set -euo pipefail

SRC_DIR="${1:-$HOME/suite-src}"
DEST_DIR="$HOME/suite-web"
BIN_NAME="suite-web"

echo "== deploy: source at $SRC_DIR, target $DEST_DIR"

# --- toolchain -------------------------------------------------------------
if ! command -v cargo >/dev/null 2>&1 && [ ! -x "$HOME/.cargo/bin/cargo" ]; then
  echo "== installing rustup"
  curl -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
fi
export PATH="$HOME/.cargo/bin:$PATH"

# rustc is sometimes missing from the stable toolchain profile — repair if needed
if ! cargo --version >/dev/null 2>&1 || ! rustc --version >/dev/null 2>&1; then
  echo "== repairing stable toolchain (installing rustc component)"
  rustup component add rustc rust-std --toolchain stable-x86_64-unknown-linux-gnu \
    || rustup toolchain install stable --profile minimal --no-self-update
fi
cargo --version && rustc --version

# --- build ------------------------------------------------------------------
echo "== building suite-web (release)"
cd "$SRC_DIR"
cargo build --release -p suite-web

# --- install -----------------------------------------------------------------
echo "== installing to $DEST_DIR"
mkdir -p "$DEST_DIR"
cp "$SRC_DIR/target/release/$BIN_NAME" "$DEST_DIR/$BIN_NAME"
chmod 755 "$DEST_DIR/$BIN_NAME"
rm -rf "$DEST_DIR/static"
mkdir -p "$DEST_DIR/static"
cp "$SRC_DIR/web/suite/static/"* "$DEST_DIR/static/"
ls -la "$DEST_DIR" "$DEST_DIR/static"

# --- systemd -------------------------------------------------------------------
if [[ "${2:-}" == "--system" ]] || [[ "${1:-}" == "--system" ]] || [[ "${SUDO_USER:-}" == "hs01" ]] && [[ "${EUID:-$(id -u)}" == "0" ]]; then
  echo "== installing systemd system service"
  cp "$SRC_DIR/deploy/suite-web.service" /etc/systemd/system/suite-web.service
  systemctl daemon-reload
  systemctl enable --now suite-web.service
  systemctl status suite-web.service --no-pager || true
else
  echo "== installing systemd user service (no root)"
  mkdir -p "$HOME/.config/systemd/user"
  cp "$SRC_DIR/deploy/suite-web.user.service" "$HOME/.config/systemd/user/suite-web.service"
  systemctl --user daemon-reload
  systemctl --user enable --now suite-web.service
  systemctl --user status suite-web.service --no-pager || true
  echo "NOTE: for the user service to start on boot without you logging in, run (needs root): loginctl enable-linger hs01"
fi

# --- health check ---------------------------------------------------------------
sleep 2
echo "== health check"
curl -s http://127.0.0.1:8760/api/health && echo || { echo "HEALTH CHECK FAILED"; exit 1; }
echo "== deploy OK"
