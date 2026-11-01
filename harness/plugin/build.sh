#!/bin/bash
# Build Plugins/RBRRecompHarness.dll with MSVC 7.1 under Wine, with --install, copy it
# as the harness plugin every play.sh run installs ($RBR_HARNESS_PLUGIN).
set -euo pipefail
source "$(dirname "$0")/../../scripts/dev-env.sh"
cd "$(dirname "$0")/../.."
toolchain=${RBR_TOOLCHAIN:?set RBR_TOOLCHAIN to the MSVC 7.1 toolchain (scripts/dev-env.sh)}
plugin=${RBR_HARNESS_PLUGIN:?set RBR_HARNESS_PLUGIN to the harness plugin (scripts/dev-env.sh)}
sdk=${RBR_PLUGIN_SDK:-original/patch-1.02/files/Main_group/Plugins/RBRTestPlugin}
[ -f "$sdk/IPlugin.h" ] || { echo "Missing plugin SDK header: $sdk/IPlugin.h (set RBR_PLUGIN_SDK)" >&2; exit 1; }
export WINEPREFIX=$toolchain/wineprefix WINEDEBUG=-all
mkdir -p build/harness
wine "$toolchain/msvc71/bin/cl.exe" /nologo /O2 /LD /MD /GR- /D__SUPPORT_PLUGIN__=1 "/I$toolchain/msvc71/include" \
  /Ibuild/harness build/harness/RBRRecompHarness.cpp /Fobuild/harness/ /Febuild/harness/RBRRecompHarness.dll \
  /link "/LIBPATH:$toolchain/msvc71/lib" kernel32.lib user32.lib >build/harness/cl.txt || { cat build/harness/cl.txt; exit 1; }
if [ "${1:-}" = "--install" ]; then
  # play.sh copies it into the slot at the start of each run
  mkdir -p "$(dirname "$plugin")"
  cp "$PWD/build/harness/RBRRecompHarness.dll" "$plugin.tmp" && mv "$plugin.tmp" "$plugin"
  echo "installed as $plugin (used by every play.sh run from now on)"
fi
