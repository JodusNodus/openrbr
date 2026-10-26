# Installing from the disc

For players on macOS or Linux with no Windows install of Richard Burns Rally. The launcher turns the disc and the 1.02
patch into a game folder: the 1.02 game with NGP, laid out as the Windows installers would leave it. It downloads NGP
from gvrc.de itself. You need nothing but the launcher.

## What you need

- The Richard Burns Rally disc: its `.iso` image, or the disc mounted or copied to a folder.
- The official 1.02 patch: `RBR_v102.zip`, or the patch `.exe` inside it.
- Optional: the NGP 7 package (`RBR_NGP-7.4.778.507.7z`), or its extracted folder. Without it, the setup downloads
  NGP 7.4.778 from gvrc.de first.
- About 3 GB free for the game.

## In the launcher

Next to the game folder's **Browse…** (at the first start, and on the **Settings** page) is **Set up from the
disc…**. It asks for:
- the disc and the 1.02 patch: type their paths or press **Choose…**; the same for NGP, or leave it empty to download it;
- the game folder, by default `~/Games/Richard Burns Rally`: it must be new or empty.

**Set up** runs it, with a progress line and **Cancel**. When it is done, the new folder becomes the game folder. An
error, such as a wrong download, shows under the buttons.

## On the command line

```sh
rerbr --install-from-disc DISC PATCH NGP OUT
```

NGP is the package's path, or `-` to download it.

For example:

```sh
rerbr --install-from-disc ~/Downloads/"Richard Burns rally.iso" ~/Downloads/RBR_v102.zip \
    ~/Downloads/RBR_NGP-7.4.778.507.7z ~/Games/"Richard Burns Rally"
```

It prints its steps and what OUT holds. Then choose OUT as the game folder ([below](#point-the-launcher-at-it)).

## What it does

The downloads are only read. The work happens in a temporary folder next to OUT, which becomes OUT at the end. After
an error or Cancel it is removed, so OUT is untouched unless the setup completes.

What OUT gets:
- the retail game files from the disc's installer, where the installer puts them: its `Root` files at the top, the
  other content folders (`Maps/`, `Cars/`, `Textures/` and the rest) under their own names; of the manuals, the
  English one only. The installer's own support files are skipped;
- the 1.02 patch's files over them, the car shaders of its `PixelShaders/Fog` and `VertexShaders/Fog` into the retail
  `fog` folders;
- NGP's files: `Plugins/` and `AudioFMOD/`, and its `physics.rbz` in place of the retail one.

The launcher reads the disc image, the installers' cabinets, the patch's self-extracting `.exe` and NGP's `.7z`
itself. It first checks a few
known files in each download and says which one is missing, so a wrong download stops before anything is written.
It was tested with one disc release (the English, French and German one); others may differ. Only the InstallShield
cabinet versions of this disc and patch (6 and later) are read.

## Point the launcher at it

After the command line, give OUT as the game folder at the first start, or later on the **Settings** page.

Setup includes `Plugins/NGP.dll`, which the launcher finds automatically. If **NGP DLL** is missing or unsupported,
use its **Browse…** button to select the 7.4.778 package's DLL.

NGP's package brings NGP's physics but no NGP cars. Add them on the **Settings** page with **Download cars…** or
**Add cars…** ([playing.md](playing.md#settings)).
