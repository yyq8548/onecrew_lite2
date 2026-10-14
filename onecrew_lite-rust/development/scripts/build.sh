#!/usr/bin/env bash
# Build the raw onecrew_lite CLI with embedded frontend and plugin assets.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
release=1
target=""
all=0
dry_run=0
install_deps=0
skip_ui=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) release=1; shift ;;
    --debug) release=0; shift ;;
    --all) all=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    --install-deps) install_deps=1; shift ;;
    --skip-ui) skip_ui=1; shift ;;
    --target) [[ $# -ge 2 && -n "$2" ]] || { echo '--target requires a Rust triple' >&2; exit 2; }; target="$2"; shift 2 ;;
    --help|-h) echo 'Usage: bash development/scripts/build.sh [--all | --target <Rust triple>] [--debug] [--dry-run] [--install-deps] [--skip-ui]'; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cd "$ROOT"
host="$(rustc -vV | sed -n 's/^host: //p')"
if [[ "$all" == 1 && -n "$target" ]]; then echo '--all and --target are mutually exclusive' >&2; exit 2; fi
if [[ "$all" == 1 ]]; then
  targets=(aarch64-apple-darwin x86_64-apple-darwin x86_64-pc-windows-msvc)
else
  targets=("${target:-$host}")
fi
command -v uv >/dev/null || { echo 'uv is required' >&2; exit 1; }
command -v node >/dev/null || { echo 'node is required' >&2; exit 1; }
command -v npm >/dev/null || { echo 'npm is required' >&2; exit 1; }
node development/scripts/sync-release-version.mjs --check
for target in "${targets[@]}"; do
  case "$target" in
    aarch64-apple-darwin|x86_64-apple-darwin|x86_64-pc-windows-msvc) ;;
    x86_64-pc-windows-gnu)
      [[ "$host" == "$target" ]] || { echo 'Windows GNU local builds require a matching native host' >&2; exit 2; } ;;
    *) echo "Unsupported distribution target: $target" >&2; exit 2 ;;
  esac
  builder=(cargo)
  if [[ "$target" == *windows-msvc && "$host" != *windows-msvc ]]; then builder+=(xwin); fi
  args=(build --locked -p onecrew --target "$target")
  if [[ "$release" == 1 ]]; then args+=(--release); fi
  if [[ "$dry_run" == 1 ]]; then printf 'Build:'; printf ' %q' "${builder[@]}" "${args[@]}"; printf '\n'; continue; fi
  if [[ "$target" == *apple-darwin && "$host" != *apple-darwin ]]; then echo 'Apple targets require a macOS build host with Xcode Command Line Tools' >&2; exit 1; fi
  if ! rustup target list --installed | grep -Fxq "$target"; then echo "Missing Rust target: run rustup target add $target" >&2; exit 1; fi
  if [[ ${#builder[@]} == 2 ]] && ! cargo xwin --version >/dev/null 2>&1; then echo 'Windows cross-build requires cargo-xwin and clang/lld; see development/README.md' >&2; exit 1; fi
  if [[ ${#builder[@]} == 2 ]]; then
    for native_tool in clang-cl lld-link llvm-lib cmake ninja nasm; do
      command -v "$native_tool" >/dev/null || {
        echo "Windows cross-build requires $native_tool on PATH; see development/README.md" >&2
        exit 1
      }
    done
  fi
done
if [[ "$dry_run" == 1 ]]; then exit 0; fi
if [[ "$skip_ui" == 0 ]]; then
  for app in apps/web apps/mcp-ui; do
    if [[ "$install_deps" == 1 ]]; then
      echo "Installing dependencies for $app (explicitly requested)..."
      npm --prefix "$app" ci --prefer-offline --no-audit --no-fund
    elif [[ ! -d "$app/node_modules" ]]; then
      echo "Missing dependencies for $app. Run npm --prefix $app ci or retry with --install-deps." >&2
      exit 1
    fi
  done
  npm --prefix apps/web run build
  npm --prefix apps/mcp-ui run build
elif [[ ! -f apps/web/dist/index.html ]]; then
  echo 'apps/web/dist/index.html is missing; remove --skip-ui or build the Web assets first.' >&2
  exit 1
fi
profile=debug
if [[ "$release" == 1 ]]; then profile=release; fi
version="$(node -p 'require("./development/release/version.json").cliPluginBase')"
for target in "${targets[@]}"; do
  builder=(cargo)
  if [[ "$target" == *windows-msvc && "$host" != *windows-msvc ]]; then builder+=(xwin); fi
  args=(build --locked -p onecrew --target "$target")
  if [[ "$release" == 1 ]]; then args+=(--release); fi
  # Keep Cargo work serial, including when building all three targets.
  if [[ "$target" == x86_64-pc-windows-gnu ]]; then
    # BoringSSL otherwise links the developer's libstdc++ DLL. The
    # service stages only the raw executable, so its runtime must be static.
    stdcpp="$(g++ -print-file-name=libstdc++.a)"
    [[ -f "$stdcpp" ]] || { echo 'Static MinGW C++ runtime is required' >&2; exit 1; }
    flags="${CARGO_ENCODED_RUSTFLAGS:-}"
    if [[ -n "$flags" ]]; then flags+=$'\x1f'; fi
    flags+="-Lnative=$(dirname "$stdcpp")"
    CC="${CC:-gcc}" CXX="${CXX:-g++}" \
      CMAKE_TOOLCHAIN_FILE="$(cygpath -m "$ROOT/development/scripts/windows-gnu.cmake")" \
      BORING_BSSL_RUST_CPPLIB_x86_64_pc_windows_gnu='static=stdc++' \
      CXXSTDLIB_x86_64_pc_windows_gnu='static=stdc++' \
      CARGO_ENCODED_RUSTFLAGS="$flags" "${builder[@]}" "${args[@]}"
  else
    "${builder[@]}" "${args[@]}"
  fi
  suffix=""
  if [[ "$target" == *windows* ]]; then suffix=.exe; fi
  binary="${CARGO_TARGET_DIR:-$ROOT/target}/$target/$profile/onecrew_lite$suffix"
  output="$ROOT/dist/onecrew_lite/$version/$target"
  if [[ "$release" == 0 ]]; then output="$ROOT/dist/onecrew_lite/debug/$version/$target"; fi
  mkdir -p "$output"
  cp -p "$binary" "$output/onecrew_lite$suffix"
  uv run development/scripts/binary-manifest.py --target "$target" --binary "$output/onecrew_lite$suffix" --output "$output/manifest.json"
  echo "Artifact directory: $output"
done
if [[ "$all" == 1 && "$release" == 1 ]]; then
  manifests=()
  for target in "${targets[@]}"; do
    manifests+=(--manifest "$ROOT/dist/onecrew_lite/$version/$target/manifest.json")
  done
  uv run development/scripts/upload-r2.py "${manifests[@]}" \
    --release-manifest "$ROOT/dist/onecrew_lite/$version/cli-manifest.json"
  echo "Release prepared: $ROOT/dist/onecrew_lite/$version (three targets, offline R2 preflight passed)"
fi
