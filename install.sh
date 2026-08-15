#!/bin/sh
set -eu

REPO="maedana/torudo"
INSTALL_DIR="${HOME}/.local/bin"

# Detect OS and architecture
OS=$(uname -s)
ARCH=$(uname -m)

case "${OS}" in
  Linux)
    case "${ARCH}" in
      x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
      *) echo "Error: unsupported architecture: ${ARCH}" >&2; exit 1 ;;
    esac
    ;;
  Darwin)
    case "${ARCH}" in
      x86_64) TARGET="x86_64-apple-darwin" ;;
      arm64)  TARGET="aarch64-apple-darwin" ;;
      *) echo "Error: unsupported architecture: ${ARCH}" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Error: unsupported OS: ${OS}" >&2
    exit 1
    ;;
esac

echo "Detected platform: ${TARGET}"

# Resolve the release tag. TORUDO_VERSION pins it; otherwise ask the GitHub API,
# then fall back to the repository's tags. Sandboxed environments often reach
# github.com but not api.github.com, so the fallback keeps the install working.
TAG="${TORUDO_VERSION:-}"

if [ -z "${TAG}" ]; then
  TAG=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')
fi

if [ -z "${TAG}" ] && command -v git >/dev/null 2>&1; then
  echo "GitHub API unavailable, falling back to the repository's tags..." >&2
  TAG=$(git ls-remote --tags --refs "https://github.com/${REPO}.git" 2>/dev/null | sed -n 's#.*refs/tags/##p' | sort -V | tail -n 1)
fi

if [ -z "${TAG}" ]; then
  echo "Error: failed to resolve the latest release tag" >&2
  echo "Set TORUDO_VERSION to install a specific release, e.g. TORUDO_VERSION=v0.17.0" >&2
  exit 1
fi

echo "Release: ${TAG}"

# Download and extract
URL="https://github.com/${REPO}/releases/download/${TAG}/torudo-${TARGET}.tar.gz"
echo "Downloading ${URL}..."

TMPDIR=$(mktemp -d)
trap 'rm -rf "${TMPDIR}"' EXIT

curl -sSL "${URL}" -o "${TMPDIR}/torudo.tar.gz"
tar xzf "${TMPDIR}/torudo.tar.gz" -C "${TMPDIR}"

# Install
mkdir -p "${INSTALL_DIR}"
mv "${TMPDIR}/torudo" "${INSTALL_DIR}/torudo"
chmod +x "${INSTALL_DIR}/torudo"

echo "Installed torudo to ${INSTALL_DIR}/torudo"

# Check PATH
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    echo "WARNING: ${INSTALL_DIR} is not in your PATH."
    echo "Add the following to your shell profile:"
    echo "  export PATH=\"${INSTALL_DIR}:\${PATH}\""
    ;;
esac
