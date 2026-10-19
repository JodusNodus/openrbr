#!/bin/bash
# Build Plugins/RBRRecompHarness.dll with MSVC 7.1 on cachy; with --install, copy it
# as the harness plugin every play.sh run installs (~/rbr-harness-plugin).
set -euo pipefail
cd "$(dirname "$0")/../.."
remote=${RBR_REMOTE:-rbr-recomp-test}
sdk=${RBR_PLUGIN_SDK:-original/patch-1.02/files/Main_group/Plugins/RBRTestPlugin}
[ -f "$sdk/IPlugin.h" ] || { echo "Missing plugin SDK header: $sdk/IPlugin.h (set RBR_PLUGIN_SDK)" >&2; exit 1; }
ssh -o BatchMode=yes cachy "mkdir -p $remote/build/harness"
rsync -a harness/plugin/RBRRecompHarness.cpp "$sdk/IPlugin.h" "$sdk/IRBRGame.h" "cachy:$remote/build/harness/"
ssh -o BatchMode=yes cachy "cd ~/$remote && export WINEPREFIX=\$HOME/rbr-recomp-test/wineprefix WINEDEBUG=-all \
  && wine toolchain/msvc71/bin/cl.exe /nologo /O2 /LD /MD /GR- /D__SUPPORT_PLUGIN__=1 /Itoolchain/msvc71/include /Ibuild/harness \
     build/harness/RBRRecompHarness.cpp /Fobuild/harness/ /Febuild/harness/RBRRecompHarness.dll \
     /link /LIBPATH:toolchain/msvc71/lib kernel32.lib user32.lib >build/harness/cl.txt || { cat build/harness/cl.txt; exit 1; }"
if [ "${1:-}" = "--install" ]; then
  # play.sh copies it into the slot at the start of each run
  ssh -o BatchMode=yes cachy "mkdir -p rbr-harness-plugin && cp $remote/build/harness/RBRRecompHarness.dll rbr-harness-plugin/RBRRecompHarness.dll.tmp && mv rbr-harness-plugin/RBRRecompHarness.dll.tmp rbr-harness-plugin/RBRRecompHarness.dll"
  echo "installed as ~/rbr-harness-plugin/RBRRecompHarness.dll (used by every play.sh run from now on)"
fi
