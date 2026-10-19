# RBRHUD gauges (RallySimFans plugin data)

RallySimFans installs ship RBRHUD (`Plugins/RBRHUD.dll`, by Towerbrah, version 0.3.5.4 on the user's install). Among
other things it gives the RSF cars working cockpit dials. The RSF car models ship their dial faces without needles;
RBRHUD draws the needles, lights and displays onto the car's interior textures each frame ("rendering onto a car
texture", its README). Only the plugin's data is read here; the DLL is not traced. Read by `rbr_formats::rsf::gauges`;
drawn by `rbr_render::dials`.

## Files

Under `Plugins/RBRHUD/`:

- `gauge_selections.ini`: one section per car folder (lower case), `gauge = <variant>` (`default` everywhere on the
  user's install). The plugin's F2 menu writes it.
- `gauges/<car folder>/<variant>/config<N>.ini`: one file per car texture the variant draws on. 99 cars, 156 configs.
- `gauges/<car folder>/textures/`: the remapped textures (`.dds`, the car's texture with the painted needles removed),
  the emissive masks (`*_em_mask.dds`), the sprite atlas (`.png` or `.dds`) and its `.ini`.
- Emissive masks: DXT5 (one DXT1, `ds3_WRC`), half the remapped texture's size. Their RGB is a coloured backlight
  pool behind the analog faces; their alpha a white glow over displays and warning lamps (the two barely overlap).
  The DXT1 mask's alpha means nothing. `rbr_render` lights the dial texture by `mask.rgb + mask.a` (RGB only for
  DXT1) at night and dusk (ours: the plugin's own blend is not traced).
- `texture_migrations.ini`: rects (as fractions) copied from the car folder's texture into the remapped one, so that a
  skin's colours show (old-format cars). One section per texture file name (case-insensitive), `migrations =
  x0,y0,x1,y1;...`. `rbr_render::dials` copies each rect from the car's texture, resampled nearest, into the remapped
  one at load.
- `rbrhud.ini`: `[GAUGES] enabled`, `replayenabled`.

## `config<N>.ini`

`[main]`: `texture` (the car texture's file name it replaces, as the car's `_shaders.ini` names it), `remapped` (the
copy it draws on, relative to the car's gauge folder), `atlas` (the atlas `.ini`), `emissive_mask`, `author`,
`description`.

Every other section has a `type`:

| `type` | Keys | Meaning (ours where the data does not say) |
| --- | --- | --- |
| `DirtyRegion` | `left`, `top`, `right`, `bottom` | A texel rect redrawn each frame (399 regions; `i20_rally2` lists none) |
| `Rotating` | `data`, `xpos`, `ypos`, `texture`, `xpivot`, `ypivot`, `rotation<N>value`, `rotation<N>deg` (2 to 5 pairs), `springconstant`, `dampingratio`, `rotationspeed` | A needle: the sprite turned about its pivot (sprite texels), which sits at `xpos, ypos`; the value maps through the pairs to degrees, clockwise in texel space; the needle sprites point left from the pivot (angle 0 points left) |
| `Static` | `xpos`, `ypos`, `texture`, `scale`, `rotation`, `colour` | A sprite at its top left corner (checked: the Ascona's `tachocentre` covers the needles' pivot) |
| `Simple` | as `Static`, `data`, `equal` or `threshold` | Shown while the value equals `equal`, else exceeds `threshold` (gear digits: `Gear` 0 R, 1 N, 2 first) |
| `Number` | `data`, `font`, `format`, `justification`, `prefix`/`suffix` (+`enabled`) | Text in an atlas font; formats `Whole`, `Decimal1`, `Decimal2`, `Padded<N>`, `PaddedSpace<N>`, `Percentage`, `TimeMinSec`, `TimeMinSecMs`, `TimeMinSecMsc`, `TimeHoursMin`, `TimeHoursMinSec`; a `prefix` with no `data` is static text |
| `Expanding` | `data`, `direction` (`right`, `left`, `up`), `expandstartvalue`/`expandendvalue` or `expandmap<N>value`/`expand` | A bar: the sprite shown up to the value's share |
| `Effect` | `effecttype` (`Blink`, `Hide`, `Show`, `ColourChange`, `Pulse`, `|`-joined), `data`, `threshold`, `equal`, `limit`, `blinkspeed` (ms), `newcolour` | Applied to the elements naming it in `effect` (comma list) while its condition holds; `limit`: under it |
| `Colour` | as `Static`, `data`, `value<N>`, `colour<N>` | A sprite tinted by the value, the colours blended per channel between the pairs |
| `Pie` | as `Static`, `data`, `rotation<N>value`, `rotation<N>deg` | The wedge of the sprite about its middle from the first angle to the value's (ours: drawn as a triangle fan) |
| `Damage` | as `Static`, `damage_telemetry` (comma list of parts), `damageleveldisp<N>` (`AARRGGBB,blink`) | The sprite coloured by the level of the worst part (level `ceil(worst * (levels - 1))`, ours), blinking (500 ms, ours) where the flag is 1. Only `124_abarth` and `i20_rally2` (mode 2) use it |

Common keys: `mode` (0 every gauge mode; 1 to 3 the pages of a digital dash, which the End key cycles), `scale`,
`rotation`, `colour` (`AARRGGBB`; the shadow copies of needles use `3c000000`). The viewer's gauge mode action
(default End) cycles the pages a config uses; mode 0 elements show on every page. Of the installed RSF cars, the
Fiesta Rally2 (RSF 81, `fiesta_rally2_23`: 1 the stage page, gear and shift ring; 2 the road page, rev dial, speed and
bars) and the GR Yaris Rally2 (107) have pages (leftovers-5, live captures).

Positions are texels of the remapped texture: every config's coordinates fit inside its remapped texture's size
(512 to 8192 texels; BMW_M3_E30's `int5.dds` is a 2048x512 transparent needle layer over the dial faces).

## Atlas `.ini`

`[MAIN] texture = textures/<atlas>.png`; `[Objects] <name>.png = x,y,w,h` (the `.dds` atlases drop the `.png`); a font
is its own section: `simplefont = x,y,w,h`, a strip of `0123456789:.-` in 13 equal cells, or one `<character code> =
x,y,w,h` line per character (proportional fonts, `yaris_rally2`'s). A `charset = x,y,w,h` key in a font section
offsets its characters' rects, which may start at x -1 (`Arial_bigdata`'s `0`; clamped at 0). Every sprite and font the 156 configs
name is in its atlas (`rbr-formats/tests/rsf_data.rs`).

## Telemetry names

By use: `RPM` 935, `Gear` 718, `UpShift` 463 (engine speed past the shift point, thresholds about -1000 to 0; ours: the car's VCU
`Gear<N>Upshift`, `DownShift` the same for `Gear<N>Downshift`),
`DistanceDriven` 120, `CoolantTemperatureC` 119, `EngineOff` 116, `CoolantTemperatureWarning` 115, `SpeedKPH` 107,
`RaceTime` 104, `OilPressureWarning` 98, `OilTemperatureC` 89, `BatteryLevelWarning` 88, `OilTemperatureWarning` 79,
`OilPressureBar` 77, `DistanceToFinish` 63, `BatteryVoltage` 56, `EngineOn` 40, `TurboPressureBar` 37, `SystemTime` 34,
`HandbrakeOn` 30, `RPMPercent` 25, and a tail of inputs, tyre health, delta and RSF stage names.
