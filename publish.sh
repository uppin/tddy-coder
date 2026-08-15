#!/usr/bin/env bash
# publish.sh — build and publish a tddy .deb to an apt repository.
#
# Usage:
#   ./publish.sh <repo-path> [--build]
#
# <repo-path> is an rsync-style destination of the apt repo root, e.g.
#   pi@192.168.1.1:apt-repo
#   /mnt/storage/apt-repo
#
# Options:
#   --build   Run ./release and './dev bun run build' before packaging.
#
# Environment overrides:
#   PUBLISH_PKG_NAME    package name      (default: tddy)
#   PUBLISH_VERSION     version           (default: parsed from packages/tddy-coder/Cargo.toml)
#   PUBLISH_ARCH        debian arch       (default: dpkg --print-architecture)
#   PUBLISH_MAINTAINER  Maintainer field  (default: "tddy <ops@indrasius.lt>")
#   PUBLISH_DEPENDS     Depends field     (default: libc6)
#   PUBLISH_INCOMING    subdir under repo (default: incoming)
#   PUBLISH_GPG_KEY_ID  if set, sign .deb with dpkg-sig (must be installed)
#
# Layout in the .deb:
#   /usr/bin/{tddy-daemon,tddy-coder,tddy-tools,tddy-remote-git-repo,codex-acp}
#   /usr/share/tddy/web/...
#   /etc/tddy/daemon.yaml          (conffile, from daemon.yaml.production)
#   /lib/systemd/system/tddy-daemon.service

set -euo pipefail
cd "$(dirname "$0")"
ROOT_DIR="$(pwd)"

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <repo-path> [--build]" >&2
  exit 1
fi

REPO_PATH="$1"; shift
WANT_BUILD=false
for arg in "$@"; do
  case "$arg" in
    --build) WANT_BUILD=true ;;
    *) echo "publish: unknown option: $arg" >&2; exit 1 ;;
  esac
done

PKG_NAME="${PUBLISH_PKG_NAME:-tddy}"
ARCH="${PUBLISH_ARCH:-$(dpkg --print-architecture)}"
MAINTAINER="${PUBLISH_MAINTAINER:-tddy <ops@indrasius.lt>}"
DEPENDS="${PUBLISH_DEPENDS:-libc6}"
INCOMING="${PUBLISH_INCOMING:-incoming}"
VERSION="${PUBLISH_VERSION:-$(grep -m1 '^version' packages/tddy-coder/Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')}"
[[ -n "$VERSION" ]] || { echo "publish: failed to determine version" >&2; exit 1; }

if "$WANT_BUILD"; then
  echo "publish: ./release"
  ./release
  echo "publish: ./dev bun run build"
  ./dev bun run build
fi

# Verify build artifacts
for f in target/release/tddy-daemon target/release/tddy-coder target/release/tddy-tools target/release/tddy-remote-git-repo; do
  [[ -f "$f" ]] || { echo "publish: missing $f (run ./release or pass --build)" >&2; exit 1; }
done
[[ -f packages/tddy-web/dist/index.html ]] || {
  echo "publish: missing packages/tddy-web/dist (run './dev bun run build' or pass --build)" >&2
  exit 1
}

# codex-acp from bun lock platform package
case "$(uname -s)" in
  Linux)  os=linux ;;
  Darwin) os=darwin ;;
  *) echo "publish: unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) cpu=x64 ;;
  aarch64|arm64) cpu=arm64 ;;
  *) echo "publish: unsupported CPU: $(uname -m)" >&2; exit 1 ;;
esac
CODEX_ACP_NATIVE="${ROOT_DIR}/node_modules/@zed-industries/codex-acp-${os}-${cpu}/bin/codex-acp"
[[ -f "$CODEX_ACP_NATIVE" ]] || {
  echo "publish: missing codex-acp native binary: $CODEX_ACP_NATIVE" >&2
  echo "publish: run './dev bun install' from repo root" >&2
  exit 1
}

# Note: when packaging on a host that differs from the target, set PUBLISH_ARCH explicitly.
echo "publish: pkg=${PKG_NAME} version=${VERSION} arch=${ARCH}"

# Stage tree
OUT_DIR="${ROOT_DIR}/target/deb"
STAGE="${OUT_DIR}/${PKG_NAME}_${VERSION}_${ARCH}"
DEB_FILE="${STAGE}.deb"
rm -rf "$STAGE"
mkdir -p \
  "${STAGE}/DEBIAN" \
  "${STAGE}/usr/bin" \
  "${STAGE}/usr/share/tddy/web" \
  "${STAGE}/etc/tddy" \
  "${STAGE}/lib/systemd/system"

# Binaries. tddy-remote-git-repo is git's GIT_SSH_COMMAND shim — a client of a daemon, not part of
# one — but it ships here so the host's operator has it on PATH without a checkout and a Rust
# toolchain. See docs/ft/daemon/remote-git-repo.md § Shipping.
install -m 0755 target/release/tddy-daemon          "${STAGE}/usr/bin/tddy-daemon"
install -m 0755 target/release/tddy-coder           "${STAGE}/usr/bin/tddy-coder"
install -m 0755 target/release/tddy-tools           "${STAGE}/usr/bin/tddy-tools"
install -m 0755 target/release/tddy-remote-git-repo "${STAGE}/usr/bin/tddy-remote-git-repo"
install -m 0755 "$CODEX_ACP_NATIVE"                 "${STAGE}/usr/bin/codex-acp"

# Web bundle
cp -a packages/tddy-web/dist/. "${STAGE}/usr/share/tddy/web/"

# daemon.yaml from template; paths fixed for FHS. Every placeholder the template defines must be
# substituted here — an unsubstituted one would leave the daemon writing to a literal
# "__DAEMON_LOG_DIR__" / "__AUTH_STORAGE_DIR__" directory at runtime.
sed -e "s#__INSTALL_BIN_DIR__#/usr/bin#g" \
    -e "s#__WEB_BUNDLE_PATH__#/usr/share/tddy/web#g" \
    -e "s#__DAEMON_LOG_DIR__#/var/log/tddy-daemon#g" \
    -e "s#__AUTH_STORAGE_DIR__#/var/lib/tddy#g" \
    "${ROOT_DIR}/daemon.yaml.production" > "${STAGE}/etc/tddy/daemon.yaml"
chmod 0644 "${STAGE}/etc/tddy/daemon.yaml"

# systemd unit
cat > "${STAGE}/lib/systemd/system/tddy-daemon.service" <<UNIT
[Unit]
Description=tddy-daemon multi-user daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/tddy-daemon -c /etc/tddy/daemon.yaml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
UNIT
chmod 0644 "${STAGE}/lib/systemd/system/tddy-daemon.service"

# DEBIAN/control
INSTALLED_SIZE=$(du -sk --exclude=DEBIAN "$STAGE" | awk '{print $1}')
cat > "${STAGE}/DEBIAN/control" <<CTRL
Package: ${PKG_NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: ${MAINTAINER}
Depends: ${DEPENDS}
Installed-Size: ${INSTALLED_SIZE}
Description: tddy — TDD-driven coder daemon, CLI, tools, and web dashboard
 Bundles tddy-daemon, tddy-coder, tddy-tools, tddy-remote-git-repo and codex-acp,
 the tddy-web static dashboard at /usr/share/tddy/web,
 a default systemd unit, and /etc/tddy/daemon.yaml.
CTRL

# conffiles
cat > "${STAGE}/DEBIAN/conffiles" <<CONF
/etc/tddy/daemon.yaml
CONF

# postinst — reload + enable + restart on configure
cat > "${STAGE}/DEBIAN/postinst" <<'POST'
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
  if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload || true
    systemctl enable tddy-daemon.service || true
    systemctl restart tddy-daemon.service || true
  fi
fi
POST
chmod 0755 "${STAGE}/DEBIAN/postinst"

# prerm — stop + disable on remove
cat > "${STAGE}/DEBIAN/prerm" <<'PRE'
#!/bin/sh
set -e
if [ "$1" = "remove" ] || [ "$1" = "deconfigure" ]; then
  if command -v systemctl >/dev/null 2>&1; then
    systemctl stop tddy-daemon.service    || true
    systemctl disable tddy-daemon.service || true
  fi
fi
PRE
chmod 0755 "${STAGE}/DEBIAN/prerm"

# Build .deb
echo "publish: building ${DEB_FILE}"
dpkg-deb --root-owner-group --build "$STAGE" "$DEB_FILE" >/dev/null

# Optional per-package signing
if [[ -n "${PUBLISH_GPG_KEY_ID:-}" ]]; then
  if command -v dpkg-sig >/dev/null 2>&1; then
    echo "publish: signing ${DEB_FILE##*/} with key ${PUBLISH_GPG_KEY_ID}"
    dpkg-sig --sign builder -k "$PUBLISH_GPG_KEY_ID" "$DEB_FILE"
  else
    echo "publish: PUBLISH_GPG_KEY_ID set but dpkg-sig not installed; skipping per-package signing" >&2
  fi
fi

# Upload
DEST="${REPO_PATH%/}/${INCOMING}/"
echo "publish: uploading ${DEB_FILE##*/} -> ${DEST}"
case "$REPO_PATH" in
  *:*)
    SSH_HOST="${REPO_PATH%%:*}"
    REMOTE_DIR="${REPO_PATH#*:}"
    ssh "$SSH_HOST" "mkdir -p '${REMOTE_DIR}/${INCOMING}'"
    rsync -avh --chmod=F644,D755 "$DEB_FILE" "$DEST"
    ;;
  *)
    mkdir -p "${REPO_PATH%/}/${INCOMING}"
    rsync -avh --chmod=F644,D755 "$DEB_FILE" "$DEST"
    ;;
esac

echo "publish: done. Refresh repo metadata on the server, e.g.:"
case "$REPO_PATH" in
  *:*) echo "  ssh ${REPO_PATH%%:*} 'cd ${REPO_PATH#*:} && reprepro processincoming default'" ;;
  *)   echo "  (cd $REPO_PATH && reprepro processincoming default)" ;;
esac
