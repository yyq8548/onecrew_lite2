#!/usr/bin/env bash
# Replace the local onecrew_lite CLI with a build.sh artifact through __bootstrap-update.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
target=""
install_path=""
library_dir=""
profile=release
plugin_install=0
dry_run=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) [[ $# -ge 2 && -n "$2" ]] || { echo '--target requires a Rust triple' >&2; exit 2; }; target="$2"; shift 2 ;;
    --install-path) [[ $# -ge 2 && -n "$2" ]] || { echo '--install-path requires an absolute path' >&2; exit 2; }; install_path="$2"; shift 2 ;;
    --library-dir) [[ $# -ge 2 && -n "$2" ]] || { echo '--library-dir requires a verified Library artifact' >&2; exit 2; }; library_dir="$2"; shift 2 ;;
    --debug) profile=debug; shift ;;
    --plugin-install) plugin_install=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    --help|-h) echo 'Usage: bash development/scripts/install-local-build.sh [--target <Rust triple>] [--install-path <absolute path>] [--library-dir <verified artifact>] [--debug] [--plugin-install] [--dry-run]'; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
cd "$ROOT"
command -v node >/dev/null || { echo 'node is required' >&2; exit 1; }
host="$(rustc -vV | sed -n 's/^host: //p')"
target="${target:-$host}"
suffix=""
if [[ "$target" == *windows* ]]; then suffix=.exe; fi
install_path="${install_path:-$HOME/.local/bin/onecrew_lite$suffix}"
[[ "$install_path" == /* || "$install_path" =~ ^[A-Za-z]:[\\/] ]] || { echo "--install-path must be absolute: $install_path" >&2; exit 2; }
version="$(node -p 'require("./development/release/version.json").cliPluginBase')"
artifact="$ROOT/dist/onecrew_lite/$version/$target"
if [[ "$profile" == debug ]]; then artifact="$ROOT/dist/onecrew_lite/debug/$version/$target"; fi
binary="$artifact/onecrew_lite$suffix"
manifest="$artifact/manifest.json"
[[ -f "$manifest" && -f "$binary" ]] || { echo "Missing artifact in $artifact; run bash development/scripts/build.sh first." >&2; exit 1; }

# Read the manifest and fingerprint the artifact as data; nothing is evaluated.
read -r m_product m_version m_target m_sha256 m_size < <(node -e '
  const m = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
  console.log([m.product, m.version, m.target, m.sha256, m.size].join(" "));
' "$manifest")
fingerprint() {
  node -e '
    const data = require("fs").readFileSync(process.argv[1]);
    console.log(require("crypto").createHash("sha256").update(data).digest("hex"), data.length);
  ' "$1"
}
read -r a_sha256 a_size < <(fingerprint "$binary")
[[ "$m_product" == onecrew_lite-cli ]] || { echo "Unexpected manifest product: $m_product" >&2; exit 1; }
[[ "$m_version" == "$version" ]] || { echo "Manifest version $m_version does not match release/version.json $version" >&2; exit 1; }
[[ "$m_target" == "$target" ]] || { echo "Manifest target $m_target does not match $target" >&2; exit 1; }
[[ "$a_sha256" == "$m_sha256" && "$a_size" == "$m_size" ]] || { echo "Artifact does not match its manifest (sha256/size); rebuild it." >&2; exit 1; }

command=("$binary" __bootstrap-update --install-path "$install_path" --expected-version "$m_version" --expected-sha256 "$m_sha256" --expected-size "$m_size")
[[ -n "$library_dir" ]] || { echo 'Matching Library artifact required: pass --library-dir; a CLI-only update can leave an incompatible running workspace.' >&2; exit 1; }
node development/scripts/install-library-companion.mjs --library-dir "$library_dir" --install-path "$install_path" --check-only
if [[ "$dry_run" == 1 ]]; then printf 'Install:'; printf ' %q' "${command[@]}"; printf '\n'; exit 0; fi
[[ "$target" == "$host" ]] || { echo "Artifact target $target cannot replace the CLI on host $host" >&2; exit 1; }
# Probe the candidate before stopping the old owner or changing installed files.
"$binary" __service-preflight
install_root="$(dirname "$install_path")"
mkdir -p "$install_root"
upgrade_lock="$install_root/.onecrew-upgrade.lock"
mkdir "$upgrade_lock" 2>/dev/null || { echo 'Another installation or interrupted installer owns .onecrew-upgrade.lock; inspect the upgrade journal before recovery.' >&2; exit 1; }
upgrade_journal="$install_root/.onecrew-upgrade.json"
upgrade_phase=prepared
previous_sha256=missing
if [[ -f "$install_path" ]]; then read -r previous_sha256 previous_size < <(fingerprint "$install_path"); fi
record_upgrade() {
  node -e '
    const fs=require("fs"); const [file,phase,oldHash,newHash,code,target,libraryFile]=process.argv.slice(1);
    const libraryBytes=fs.readFileSync(libraryFile);
    const library=JSON.parse(libraryBytes);
    let previous; try { previous=JSON.parse(fs.readFileSync(file,"utf8")); } catch(error) { if(error.code!=="ENOENT") throw error; }
    if (previous && previous.schema_version !== 1) throw new Error("unsupported_upgrade_journal_version");
    const same=previous?.candidate_cli_sha256===newHash;
    const lineage=previous?.previous_generations||[];
    if(previous&&!same) { const {previous_generations,...generation}=previous; lineage.push(generation); }
    const timestamp=new Date().toISOString();
    const value={schema_version:1,phase,target,previous_cli_sha256:same?previous.previous_cli_sha256:oldHash,candidate_cli_sha256:newHash,expected_plugin_binary_sha256:newHash,
      library:{version:library.version,target:library.target,manifest_sha256:require("crypto").createHash("sha256").update(libraryBytes).digest("hex")},exit_code:Number(code),updated_at:timestamp,previous_generations:lineage,
      history:[...(same?(previous.history||[]):[]),{phase,exit_code:Number(code),at:timestamp}]};
    fs.writeFileSync(file+".tmp",JSON.stringify(value)+"\n"); fs.renameSync(file+".tmp",file);
  ' "$upgrade_journal" "$upgrade_phase" "$previous_sha256" "$m_sha256" "$1" "$m_target" "$library_dir/manifest.json"
}
finish_upgrade() {
  result=$?
  if [[ "$result" != 0 ]]; then upgrade_phase="failed:$upgrade_phase"; fi
  record_upgrade "$result" || true
  rmdir "$upgrade_lock"
}
trap finish_upgrade EXIT
record_upgrade 0
upgrade_phase=cli_replacement
record_upgrade 0
"${command[@]}"

read -r i_sha256 i_size < <(fingerprint "$install_path")
installed_version="$("$install_path" --version)"
[[ "$installed_version" == "onecrew_lite $m_version" ]] || { echo "Installed CLI reports '$installed_version', expected 'onecrew_lite $m_version'" >&2; exit 1; }
[[ "$i_sha256" == "$m_sha256" && "$i_size" == "$m_size" ]] || { echo "Installed CLI at $install_path does not match the manifest (sha256/size)" >&2; exit 1; }
echo "Installed $installed_version ($target) at $install_path; sha256 $m_sha256, $m_size bytes."
if [[ -n "$library_dir" ]]; then
  upgrade_phase=library_installation
  record_upgrade 0
  node development/scripts/install-library-companion.mjs --library-dir "$library_dir" --install-path "$install_path"
fi
upgrade_phase=service_host_registration
record_upgrade 0
if [[ "$target" == *windows* ]]; then "$install_path" service host install; fi
if [[ "$plugin_install" == 1 ]]; then
  # Establish the shared owner from the installation host before Codex probes
  # the plugin in temporary Windows Jobs. Keep its output off installer pipes.
  startup_log="$(dirname "$install_path")/service-startup.log"
  upgrade_phase=service_start
  record_upgrade 0
  if ! "$install_path" web --no-browser >"$startup_log" 2>&1; then
    cat "$startup_log" >&2
    echo 'Shared service startup failed; plugin installation was not started.' >&2
    exit 1
  fi
  cat "$startup_log"
  upgrade_phase=plugin_installation
  record_upgrade 0
  "$install_path" plugin install
  upgrade_phase=installed_pending_live_verification
  echo 'Files installed; live MCP and workspace verification are still pending. Reload the Codex plugin connection once if the host still uses the previous generation.'
else
  upgrade_phase=cli_library_installed_plugin_pending
  echo "Next: $install_path plugin install, then fully quit and restart Codex."
fi
