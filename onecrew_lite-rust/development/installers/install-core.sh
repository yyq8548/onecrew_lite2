#!/bin/sh
# Generated with verified component fingerprints. No client registration, login,
# browser launch, dependency download or business-data migration occurs here.
set -eu
umask 077
SOURCE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
VERSION='__VERSION__'
TARGET='__TARGET__'
CLI_SHA='__CLI_SHA__'
CLI_SIZE='__CLI_SIZE__'
LIB_SHA='__LIB_SHA__'
LIB_SIZE='__LIB_SIZE__'
BIN="$HOME/.local/bin"
CLI="$BIN/onecrew_lite"
LIB="$BIN/onecrew_lite_library"
TX="$BIN/.onecrew_lite-install"
fail() { printf '%s\n' "$1" >&2; exit 1; }
fingerprint() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
verify() {
  [ -f "$1" ] && [ ! -L "$1" ] &&
    [ "$(/usr/bin/wc -c < "$1" | /usr/bin/tr -d ' ')" = "$3" ] &&
    [ "$(fingerprint "$1")" = "$2" ]
}
[ "$#" -eq 0 ] || fail 'install-core.sh takes no options.'
[ "$(/usr/bin/uname -s)" = Darwin ] || fail 'Unsupported OS.'
ARCH=$(/usr/bin/uname -m)
if [ "$(/usr/sbin/sysctl -in hw.optional.arm64 2>/dev/null || :)" = 1 ]; then ARCH=arm64; fi
case "$TARGET:$ARCH" in aarch64-apple-darwin:arm64|x86_64-apple-darwin:x86_64) ;; *) fail 'Unsupported native architecture.' ;; esac
/usr/bin/sw_vers -productVersion | /usr/bin/awk -F. '{exit !($1>13 || ($1==13 && $2>=5))}' || fail 'macOS 13.5 or newer is required.'
case "$HOME" in /*) ;; *) fail 'Absolute user home required.' ;; esac
for p in "$HOME" "$HOME/.local" "$BIN" "$CLI" "$LIB" "$TX"; do
  [ ! -L "$p" ] || fail 'Symlink installation path rejected.'
done
verify "$SOURCE/onecrew_lite" "$CLI_SHA" "$CLI_SIZE" || fail 'CLI integrity check failed.'
verify "$SOURCE/onecrew_lite_library/onecrew_lite" "$LIB_SHA" "$LIB_SIZE" || fail 'Library integrity check failed.'
# Never execute an unverified downloaded candidate, including --version.
[ "$("$SOURCE/onecrew_lite" --version)" = "onecrew_lite $VERSION" ] || fail 'CLI version mismatch.'
/bin/mkdir -p "$BIN"
/bin/mkdir "$TX" 2>/dev/null || fail 'onecrew_lite installation busy or interrupted. Preserve .onecrew_lite-install for recovery; do not delete it blindly.'
PHASE=preparing
OLD_VERSION=''
OLD_SHA=''
OLD_SIZE=''
CLI_CHANGED=0
LIB_CHANGED=0
HAD_LIBRARY=0
finish() {
  code=$?
  trap - EXIT HUP INT TERM
  if [ "$code" -ne 0 ] && [ "$CLI_CHANGED" -eq 1 ]; then
    # Bootstrap owns service draining and CLI replacement, including rollback.
    if [ -f "$TX/previous-cli" ]; then
      if ! "$TX/previous-cli" __bootstrap-update --expected-version "$OLD_VERSION" --expected-sha256 "$OLD_SHA" --expected-size "$OLD_SIZE" --install-path "$CLI" --json >/dev/null; then
        printf 'Recovery required: CLI restore failed. Keep %s\n' "$TX" >&2
        exit 1
      fi
    else
      /bin/rm -f "$CLI" || exit 1
    fi
  fi
  if [ "$code" -ne 0 ] && [ "$LIB_CHANGED" -eq 1 ]; then
    if [ -d "$TX/previous-library" ] || [ "$HAD_LIBRARY" -eq 0 ]; then
      if [ -d "$LIB" ]; then /bin/mv "$LIB" "$TX/failed-library" || exit 1; fi
      if [ -d "$TX/previous-library" ]; then /bin/mv "$TX/previous-library" "$LIB" || exit 1; fi
    fi
  fi
  if [ "$code" -ne 0 ]; then
    printf 'onecrew_lite core installation failed at %s; previous files restored where replacement began.\n' "$PHASE" >&2
  fi
  /bin/rm -rf "$TX"
  exit "$code"
}
trap finish EXIT
trap 'exit 1' HUP INT TERM
phase() { PHASE=$1; printf '%s\n' "$PHASE" > "$TX/phase"; }
if [ -e "$CLI" ]; then
  [ -f "$CLI" ] || fail 'Existing CLI is not a regular file.'
  /bin/cp -p "$CLI" "$TX/previous-cli"
  OLD_VERSION=$("$TX/previous-cli" --version)
  case "$OLD_VERSION" in 'onecrew_lite '*) OLD_VERSION=${OLD_VERSION#onecrew_lite } ;; *) fail 'Unexpected existing CLI identity.' ;; esac
  OLD_SHA=$(fingerprint "$TX/previous-cli")
  OLD_SIZE=$(/usr/bin/wc -c < "$TX/previous-cli" | /usr/bin/tr -d ' ')
fi
if [ -e "$LIB" ]; then
  HAD_LIBRARY=1
  [ -d "$LIB" ] && [ -f "$LIB/onecrew_lite" ] && [ ! -L "$LIB/onecrew_lite" ] || fail 'Unexpected Library installation.'
fi
/bin/mkdir "$TX/next-library"
/bin/cp -R "$SOURCE/onecrew_lite_library/." "$TX/next-library/"
/bin/chmod 755 "$TX/next-library/onecrew_lite"
verify "$TX/next-library/onecrew_lite" "$LIB_SHA" "$LIB_SIZE" || fail 'Staged Library integrity mismatch.'
phase stopping_library
if [ -f "$LIB/onecrew_lite" ]; then "$LIB/onecrew_lite" stop >/dev/null; fi
phase replacing_cli
CLI_CHANGED=1
"$SOURCE/onecrew_lite" __bootstrap-update --expected-version "$VERSION" --expected-sha256 "$CLI_SHA" --expected-size "$CLI_SIZE" --install-path "$CLI" --json >/dev/null
phase replacing_library
LIB_CHANGED=1
if [ -d "$LIB" ]; then /bin/mv "$LIB" "$TX/previous-library"; fi
/bin/mv "$TX/next-library" "$LIB"
phase verifying
verify "$CLI" "$CLI_SHA" "$CLI_SIZE" || fail 'Installed CLI integrity mismatch.'
verify "$LIB/onecrew_lite" "$LIB_SHA" "$LIB_SIZE" || fail 'Installed Library integrity mismatch.'
[ "$("$CLI" --version)" = "onecrew_lite $VERSION" ] || fail 'Installed CLI version mismatch.'
phase complete
printf 'onecrew_lite %s core installed: %s\n' "$VERSION" "$CLI"
printf 'Client connection, dependencies, login and browser readiness have not been verified.\n'
