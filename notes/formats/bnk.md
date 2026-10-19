# Menu graphics bank (`Generic/PC_Generic_<width>.bnk`)

One bank per menu resolution (640, 800, 1024, 1280), read by `CPC_GraphicsDaemon::LoadMenuBank` (0x4c1100) for the
snapped resolution. Reader: `rbr_formats::bnk`.

```text
u32 page_count, u32 sprite_count (0x54d in every retail bank)
page_count x char[64]    page file in Generic\ (NUL, then 0xff padding), a DXT .dds
sprite_count x 36 bytes  u32 page; f32 u[4]; f32 v[4]  (corners TL, TR, BL, BR, in 0..1 of the page)
```

A sprite number is the same in every bank. Known sprites:

| Sprite | What | Used by |
| --- | --- | --- |
| 0x520-0x537 | stage pictures, 4 per rally: 10-13, 20-23, 31-34, 41-44, 51-54, 61-64 | `CScreenshotUI::Vf0C` (0x53f7d0), on the map's base map (`E52ModeHost::Mode` 0x4a1560) |
| 0x538 | map 71's picture | same |
| 0x4df | warning sign, the picture of every other map | same |
| 0x4c9 | drawn over the picture in some menu states (condition `FUN_00599a10`, not traced) | same |

The 1280 bank's stage pictures are 384x184; the 1024 bank's 308x150. The launcher's stage preview uses them
(`rust/rbr-launcher/src/preview.rs`).
