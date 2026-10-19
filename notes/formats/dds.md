# DDS textures in stage archives

Reader: `rust/rbr-formats/src/dds.rs`. Test: `all_retail_dds_parse` (`RBR_DATA`).

Scope: the `.dds` entries of `Maps/*_textures.rbz` (88 archives, `Clouds_textures.rbz` included), which the stage
`[TextureInfo]` names resolve to (`notes/formats/lbs.md`, Textures). Retail 1.0: 3,485 files, 2,217 `DXT1` (BC1)
and 1,268 `DXT3` (BC2), 13,940 mip levels, widest 2048 px, all dimensions multiples of four. Community stages add
`DXT5` (BC3), 32-bit RGBA and sizes under one block (`notes/formats/rsf-stages.md`).

## Header (verified on all 3,485 files)

Standard DDS: `DDS ` magic, then the 124-byte header (`dwSize` = 124, `DDS_PIXELFORMAT` `dwSize` = 32, FourCC
flag set). Fields read: `dwHeight` at 12, `dwWidth` at 16, `dwMipMapCount` at 28, FourCC at 84.

- Retail files do not set the `DDSD_MIPMAPCOUNT` flag (`dwFlags` 0x20000) but store the count, 4 in every file.
  The reader takes `dwMipMapCount` whenever it is non-zero.
- No DX10 extension header.

## Mip chain

Levels follow the header from the largest, each stored row by row with no padding. A level is
`ceil(w/4) * ceil(h/4)` blocks (at least one per dimension), 8 bytes per block for DXT1 and 16 for DXT3. The file
ends after the last level: every retail file matches the full chain exactly (the reader requires it).
