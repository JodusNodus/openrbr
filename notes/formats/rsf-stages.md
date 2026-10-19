# RallySimFans stage folders

RallySimFans (RSF) installs community stages next to the retail ones, one folder per stage. The Rust readers take
them through `rbr_formats::rsf` (folder, weathers, problems) and `rbr_formats::stage_files` (a stage name is a retail
track name or the path of a folder stage's files without the extension). Checked on six samples (101, 129, 138, 140,
294, 450; `RBR_RSF`, test `rbr-formats/tests/rsf_data.rs`) and RSF's `Maps/` root files.

## Folder layout

`Maps/<id>-<name>/`:

| File | Role |
|---|---|
| `Tracks<id>.ini` | `[Map<id>]`: `TrackName = "Maps\<id>-<name>\track-<id>"`, `Particles`, `StageName`, `Surface`, `Length`, `Author`, `Country`, `DefaultWetness`, `AnimationPath`. Same keys as retail `Tracks.ini`. |
| `track-<id>_<L>.{lbs,col,trk,dls,mat,fnc,ini}` | one set per variant letter `L` (M, N, E, O, S), as retail. |
| `track-<id>_<L>_textures.rbz` and/or `track-<id>_<L>_textures/` | textures, zipped or loose. |
| `TextureFilenameMap<id>.ini` | texture remap (below). |
| `TrackSettings<id>.ini` | the stage's weathers and lighting (below). |
| `CustomData/<AnimationPath>/Scripts/*.lua` | the stage's own character scripts, over retail `Scripts.rbz` (ours: loaded by `characters::load`). |
| `PS_*.ini`, `<id>-*.dds` | the stage's particle file, banner and loading images. |

`Maps/` itself holds RSF's `TrackSettings<id>.ini` copies (101's equals the folder's), `PS_*.ini` particle files
(an RSF particle mod over retail), `Tracks.ini` (`[General]` only), `TextureFilenameMap00.ini` and the retail-style
`track-*.ini` (85 of 86 equal retail; `track-56_O.ini` differs). `rsfdata/cache/stages_data.json` lists every stage:
`id`, `name`, `surface_id` (1 tarmac, 2 gravel, 3 snow), `length`, `deftime`, `author`. Ours takes the surface from
`surface_id`, else from `Tracks<id>.ini` `Surface` (0 tarmac, 1 gravel, 2 snow; inferred from the samples against
`surface_id`), and the name from `StageName`, else the list, else the folder.

## Encryption

Some stages are protected: their files start with `01 a0 00 00` (`.ini`: `01 a1 00 00`) and the rest is
high-entropy. Samples 294 and 450 have encrypted `.lbs`, `.col`, `.ini`, `_textures.rbz` and `TextureFilenameMap`;
their `.trk`, `.dls`, `.mat`, `.fnc`, `Tracks<id>.ini`, `TrackSettings<id>.ini` and particle files are plain. 294 also
ships a plain `track-294_<L>.lb2` (47 MB, parses with `lbs::parse`) beside its 1.15 MB encrypted `.lbs`; what reads
it is not known, and the stage stays unloadable on its `.col`, `.ini` and textures. On cachy
(lead count, first `.col` per folder) 162 of 443 folders are encrypted. We do not decrypt them (lead decision): the
check is per file (`stage_files::encrypted`), `rsf::Stage::check` names the first encrypted required file, and the
standalone, viewer and launcher show that reason.

## Format differences from retail

- **`.col`**: two cells can reach the same root placeholder (129, cell 10). The loader grafts in file order, so the
  later cell wins; `col::parse` no longer rejects it (`notes/formats/col.md`, Graft).
- **`.mat`**: may end in a `CMAP` block after the entries (129, 140, 294). `E09_LoadMaterials` reads `count` entries
  and ignores the rest; `MatFile::tail` keeps it.
- **`.lbs` `(2,11)` start location**: community exporters leave the 4x4 matrix zero. The loader skips the matrix and
  reads position and rotation, so the start pose is built from the rotation (`rbr-render` `start_pose`).
- **`.dds`**: DXT5 (151 sample textures) and one 32-bit RGBA texture (`track-138_M_textures/dry/toiture.dds`,
  masks R `0xff`, G `0xff00`, B `0xff0000`, A `0xff000000`). Some are smaller than one 4x4 block (2x2); the
  viewer decodes those to RGBA.
- **`.ini` sections**: a section header can be followed by a `;` comment.
- **`PS_*.ini`**: integer fields can be written as floats (`1.0` in `PS_Arctic.ini`); the game reads them atoi-style.
  `PS_BTBTarmac.ini` lists 116 material aliases against a declared 112, so its later sections read shifted.

## Texture remap (`TextureFilenameMap<id>.ini`)

Text, one `requested<TAB>target` per line, `;` comments, overlaying retail `TextureFilenameMap.dat`. Targets name
another stage's archive (`track-13_o_textures\dry\a.dds`), often a retail one or a sibling RSF stage's (140 uses
138's). A target without an archive (`damp\bka.dds`, 1,555 of 129's) is in the request's own archive (ours, from the
files). Lookup order for an archive: the stage's folder, sibling RSF stage folders, RSF `Maps/`, retail `Maps/`; each
as `.rbz` or a loose `_textures` folder.

## Base track (ours, inferred)

192 of the 281 plain folders hold an empty file `track-<id>_<base>_<name>` (`103-Falstone II` has
`track-103_21_Falstone II`, `200-Cote D'Arbroz Gravel` has `track-200_41_...`): the retail track the stage was built
on. Their remaps cover the textures RSF's tool found identical, but not the dry ones retail shares through
`TextureFilenameMap.dat` (`track-21_o_textures\dry\fence01.dds` -> `...\damp\fence01.dds`), so the stage's
`track-103_o_textures\dry\fence01.dds` is in no archive and no map. Ours (`rsf::base_track`, `rbr-render`
`Stage::load_textures`): a request that the stage's own lookup does not find is tried as the base track's
(`track-21_o_textures\dry\fence01.dds`) through the same map. In the cachy sweep this supplied every missing texture
of those stages (`lead/reports/rsf-sweep.md`). What RSF's plugin does is not traced.

Some stages also leave textures in another variant's archive (157's `O` archive holds only shadows; its textures are
in `M`'s, and the remap lists only the `wet` ones) or only in another weather's folder. Ours, after the base track: the
same file in the stage's other weather folders, then in each other variant's (`rbr-render` `other_folders`).

## Texture formats

Besides DXT1/3/5 and the 32-bit RGBA file: 5 `X8R8G8B8` (32 bit, no alpha), 3 `A8R8G8B8` and 1 `R8G8B8` (24 bit)
textures in the 281 plain folders. `dds::to_rgba8` rewrites them as the RGBA file before the parse (alpha 255 for the
X and 24-bit ones). 4 `.dds` files of stage 384 are JPEG files (D3DX reads them by content); we have no JPEG decoder,
so they stay missing.

## Weathers (`TrackSettings<id>.ini`)

Section names are `<id><L>_[<CL>_]<preset>_<cloud>`: variant letter, optional cloud letter, a `PRESETS` word
(`crisp`, `hazy`, `norain`, `lightrain`, `heavyrain`, `nosnow`, `lightsnow`, `heavysnow`, `lightfog`, `heavyfog`) and
a `CLOUDS` word. Each section is one season-like row (`Season::of_rsf`, ours):

- good draws are the rows without rain, snow or fog, bad draws the rest (each takes the other's when empty);
- wetness is 2 for `heavyrain`, 1 for `lightrain`, else `DefaultWetness`, else 0;
- the row's letter picks the variant, so the weather chooses the stage files (129 bad is `_O`, damp).

`rsfdata/cache/stages_tracksettings.json` has the same rows as fields: `timeofday` (letter index M N E O S),
`timeofday2` (cloud letter), `skytype` (preset id) and `skycloudtype` (cloud id). When nothing picks a variant
(viewer), it is the first of O, M, N, E, S that is installed and has a weather.

## Records

The game's records and times tables have 37 rows; community stages get theirs in `community-records.ini` next to the
retail records (ours). `times.ini` has no row for them, so the reference time is the missing-times default; RSF's
`deftime` is not used.

## Missing character scripts

129 places items with `LOAD_SPECTATOR_standard_02/04/10/11.lua`, which neither retail `Scripts.rbz` nor RSF's install
has (lead searched cachy). The game drops an item whose script does not resolve, without a log
(`notes/stage-characters.md`, `E01ResolveA`); ours drops it the same way.
