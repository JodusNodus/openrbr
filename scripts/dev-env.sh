# Sourced by the scripts (and run by harness/dev_env.py): loads dev.env and fills in the development folders.
# dev.env is this checkout's, else the main checkout's (worktrees share it); KEY=value lines, `#` comments, `~` and
# `$VAR` expanded. A variable already set in the environment wins over the file. dev.env.example lists the keys.
# shellcheck shell=bash

_dev_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
_dev_file=$_dev_root/dev.env
if [[ ! -f $_dev_file ]]; then
    _dev_common=$(git -C "$_dev_root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) &&
        _dev_file=$(dirname "$_dev_common")/dev.env
fi
if [[ -f $_dev_file ]]; then
    while IFS= read -r _dev_line || [[ -n $_dev_line ]]; do
        _dev_line=${_dev_line%%#*}
        [[ $_dev_line =~ ^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]] || continue
        _dev_key=${BASH_REMATCH[1]}
        _dev_value=${BASH_REMATCH[2]%"${BASH_REMATCH[2]##*[![:space:]]}"}
        [[ -n ${!_dev_key+set} || -z $_dev_value ]] && continue
        _dev_value=${_dev_value/#\~/$HOME}
        _dev_value=$(eval "printf '%s' \"$_dev_value\"")
        export "$_dev_key=$_dev_value"
    done <"$_dev_file"
fi

# The harness's folders, each overridable: the original games it runs under Wine, their prefixes, the plugin
# toolchain and the runs' output.
export RBR_DEV=${RBR_DEV:-$HOME/rerbr-dev}
export RBR_GAME=${RBR_GAME:-$RBR_DEV/game}                  # vanilla 1.02 install (slots: -s<N>, record.sh: -rec)
export RBR_PREFIX=${RBR_PREFIX:-$RBR_DEV/prefix}            # its Wine prefix (the same suffixes)
export RBR_NGP_GAME=${RBR_NGP_GAME:-$RBR_DEV/ngp-game}      # NGP 7.4.778 install
export RBR_NGP_PREFIX=${RBR_NGP_PREFIX:-$RBR_DEV/ngp-prefix}
export RBR_RSF_PREFIX=${RBR_RSF_PREFIX:-$RBR_DEV/rsf-prefix} # the Wine prefix RBR_RSF's game runs in
export RBR_TOOLCHAIN=${RBR_TOOLCHAIN:-$RBR_DEV/toolchain}  # MSVC 7.1 (msvc71/) and its Wine prefix (wineprefix/)
export RBR_RUNS=${RBR_RUNS:-$RBR_DEV/runs}                  # harness recordings, playbacks, snapshots
export RBR_NGP_RUNS=${RBR_NGP_RUNS:-$RBR_DEV/ngp-runs}
export RBR_RSF_RUNS=${RBR_RSF_RUNS:-$RBR_DEV/rsf-runs}
export RBR_HARNESS_PLUGIN=${RBR_HARNESS_PLUGIN:-$RBR_DEV/plugin/RBRRecompHarness.dll} # what play.sh installs
export RBR_RSF_PLUGIN=${RBR_RSF_PLUGIN:-$RBR_DEV/rsf-plugin} # rsf-session.sh's plugin build
export RBR_APITRACE=${RBR_APITRACE:-$RBR_DEV/apitrace}
export RBR_FFB_TRACE=${RBR_FFB_TRACE:-$RBR_DEV/ffb-trace}

unset _dev_root _dev_file _dev_common _dev_line _dev_key _dev_value
