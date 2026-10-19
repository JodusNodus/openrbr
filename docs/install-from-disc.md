# Installing from the disc

For players on macOS or Linux with no Windows install of Richard Burns Rally. The launcher turns three files you own
into a game folder: the 1.02 game with NGP, laid out as the Windows installers would leave it. You need nothing but
the launcher.

## What you need

- The Richard Burns Rally disc: its `.iso` image, or the disc mounted or copied to a folder.
- The official 1.02 patch: `RBR_v102.zip`, or the patch `.exe` inside it.
- The NGP 7 package (`RBR_NGP-7.4.778.507.7z`), or its extracted folder.
- About 3 GB free for the game.

## In the launcher

Next to the game folder's **Browse…** (at the first start, and on the **Settings** page) is **Set up from the
disc…**. It asks for:
- the disc, the 1.02 patch and the NGP package: type their paths or press **Choose…**;
- the game folder, by default `~/Games/Richard Burns Rally`: it must be new or empty.

**Set up** runs it, with a progress line and **Cancel**. When it is done, the new folder becomes the game folder. An
error, such as a wrong download, shows under the buttons.

## On the command line

```sh
rerbr --install-from-disc DISC PATCH NGP OUT
```

For example:

```sh
rerbr --install-from-disc ~/Downloads/"Richard Burns rally.iso" ~/Downloads/RBR_v102.zip \
    ~/Downloads/RBR_NGP-7.4.778.507.7z ~/Games/"Richard Burns Rally"
```

It prints its steps and what OUT holds. Then choose OUT as the game folder ([below](#point-the-launcher-at-it)).

## What it does

The downloads are only read. The work happens in a temporary folder next to OUT, which becomes OUT at the end. After
an error or Cancel it is removed, so OUT is untouched unless the setup completes. It took about 7 seconds on the Mac
it was tested on.

What OUT gets:
- the retail game files from the disc's installer, where the installer puts them: its `Root` files at the top, the
  other content folders (`Maps/`, `Cars/`, `Textures/` and the rest) under their own names; of the manuals, the
  English one only. The installer's own support files are skipped;
- the 1.02 patch's files over them, the car shaders of its `PixelShaders/Fog` and `VertexShaders/Fog` into the retail
  `fog` folders;
- NGP's files: `Plugins/` and `AudioFMOD/`, and its `physics.rbz` in place of the retail one.

The launcher reads the disc image, the installers' cabinets, the patch's self-extracting `.exe` and NGP's `.7z`
itself (`rbr_formats::iso9660`, `rbr_formats::installshield`, and the `sevenz-rust2` crate). It first checks a few
known files in each download and says which one is missing, so a wrong download stops before anything is written.
It was tested with one disc release (the English, French and German one); others may differ. Only the InstallShield
cabinet versions of this disc and patch (6 and later) are read.

## Point the launcher at it

The button sets the game folder itself. After the command line, start the launcher: at the first start it asks for
the game folder, so give it OUT. Later, the **Settings** page changes it. There **Ready to drive** ticks the game
folder and NGP 7.4.778, and shows the cars missing, with **Add cars…**.

NGP's package brings NGP's physics but no NGP cars ([ngp.md](../notes/ngp.md#package-and-local-data)). Add them
with **Add cars…** from gvrc.de's NGP car packs ([README](../README.md#3-cars)).
