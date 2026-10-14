#!/bin/sh
set -eu

VERSION="0.19.24"
INSTALL_DIR="${SKILLSHARE_INSTALL_DIR:-${HOME}/.local/bin}"

if [ -x "${INSTALL_DIR}/skillshare" ] &&
  "${INSTALL_DIR}/skillshare" version 2>/dev/null | grep -F "${VERSION}" >/dev/null; then
  echo "Skillshare ${VERSION} is already installed at ${INSTALL_DIR}/skillshare"
  exit 0
fi

case "$(uname -s)" in
  Darwin) os="darwin" ;;
  Linux) os="linux" ;;
  *)
    echo "Unsupported OS. Install Skillshare with its official platform installer." >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  arm64|aarch64) arch="arm64" ;;
  x86_64|amd64) arch="amd64" ;;
  *)
    echo "Unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

asset="skillshare_${VERSION}_${os}_${arch}.tar.gz"
case "${os}/${arch}" in
  darwin/arm64) expected="4f0601a75f5e9ee256f7f2194948701dac4fc2d428783f65dd13711362ea8173" ;;
  darwin/amd64) expected="2bc203dd3516e65cc0bf5e146631e4c600e82f17b14e8e6818206b9e266028b0" ;;
  linux/arm64) expected="37e14b46228a9613b812fa98feed5261e8bb1411cbd3f3a525322a2c1341b274" ;;
  linux/amd64) expected="8ae991848e5e0c3bb7c90585b3f30c8e5f2eda0c8a31d474453f2fc484de3dbc" ;;
esac

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT HUP INT TERM
archive="${tmp_dir}/${asset}"
url="https://github.com/runkids/skillshare/releases/download/v${VERSION}/${asset}"

echo "Downloading Skillshare ${VERSION} for ${os}/${arch}..."
curl --fail --location --silent --show-error "${url}" --output "${archive}"

if command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "${archive}" | awk '{print $1}')"
elif command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "${archive}" | awk '{print $1}')"
else
  echo "Neither shasum nor sha256sum is available." >&2
  exit 1
fi

if [ "${actual}" != "${expected}" ]; then
  echo "Skillshare archive checksum mismatch." >&2
  exit 1
fi

tar -xzf "${archive}" -C "${tmp_dir}"
test -x "${tmp_dir}/skillshare" || chmod +x "${tmp_dir}/skillshare"
"${tmp_dir}/skillshare" version | grep -F "${VERSION}" >/dev/null

mkdir -p "${INSTALL_DIR}"
staged="${INSTALL_DIR}/.skillshare.new.$$"
cp "${tmp_dir}/skillshare" "${staged}"
chmod 0755 "${staged}"
mv -f "${staged}" "${INSTALL_DIR}/skillshare"

echo "Installed Skillshare ${VERSION} at ${INSTALL_DIR}/skillshare"
echo "Ensure ${INSTALL_DIR} is on PATH."
