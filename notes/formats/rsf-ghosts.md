# RSF ghost cars (`GhostCars/*.gch`)

RallySimFans keeps its own ghosts, not the game's `.ghc`. Read from the sample install on cachy
(`~/Games/Richard Burns Rally/GhostCars`, 510 files; maps 129, 429, 450 and 476 copied to `rsf-samples/GhostCars`).
RSF's plugin is not traced. Parser and layout: `rust/rbr-formats/src/rsf/ghosts.rs`.

## Layout

```text
GhostCars/<map>/<map>_latest.gch                the last drive on the map
GhostCars/<map>/<Dry|Damp|Wet>/<map>.gch         the best drive on the map in that condition
GhostCars/<map>/<Dry|Damp|Wet>/<map>_<car>.gch   the best of each RSF car id (cars.json)
GhostCars/<map>/<Dry|Damp|Wet>/<map>_<group>.gch the best of each group: the car's base group (cars.json
                                                 base_group_id, cargroups.json name: "Group A8", "Rally 3")
```

A best file is a byte copy of the drive that set it (`476.gch` = `476_94.gch` = `476_Group A7.gch`). In every sample
folder the overall best is the lowest finish time of the folder.

## `.gch`

Little-endian. Header 0x30 bytes:

| Offset | Type | Value |
| --- | --- | --- |
| 0x00 | 4 bytes | `01GC` |
| 0x04 | u32 | map |
| 0x08 | u32 | 5 in every file (RSF's car slot, inferred) |
| 0x0c | u32 | RSF car id |
| 0x10 | u32 x3 | 0 0 0, or 0 1 0 in every `Damp` file and in some `Dry` ones (the middle word is probably the wetness; why damp drives sit in `Dry/` is not known) |
| 0x1c | f32 x2 | split 1, split 2 (s) |
| 0x24 | f32 | finish time (s), penalties included |
| 0x28 | f32 | penalty (s) |
| 0x2c | u32 | sample count |

Then 40-byte samples, about every 100 ms (100 to 109 ms apart):

| Offset | Type | Value |
| --- | --- | --- |
| 0x00 | u32 | ms since the race clock's state 1 (the car leaves the line about 7 s in, as in the game) |
| 0x04 | f32 | distance along the stage, whole metres |
| 0x08 | f32 | the race clock, whole seconds: 0 through the countdown, then counting, jumping by a penalty when it is given |
| 0x0c | f32 x4 | rotation quaternion `[x, y, z, w]`, physics frame |
| 0x1c | f32 x3 | position, physics frame |

Then 16 bytes that are no MD5, SHA-1, SHA-256, BLAKE2 or CRC32 of the file or its parts (probably a signature).

### The position

The point is the body position (the `CCar` +0x140 our recordings keep, `rbr_recording::pose`) raised by about
0.43 m, whatever the car. Measured with `rbr-standalone --autopilot --boot --car 5 --ngp-physics <NGP 7.4.778>
--rsf-car <id> --rsf-stage 129` (the body at rest before the start, `drive_path`):

- On map 129's start line the first sample agrees with our body in x and y within 2 cm (Fiesta Rally3, 88:
  0.0015 and 0.015 m) and lies 0.418 m (88) and 0.447 m (Xsara Kit Car, 94) above it. The model root would be
  1.45 to 1.57 m off along the car.
- Along the drive the height holds: our autopilot drive of the Fiesta on 129 against `129_88.gch` and `129.gch`, at
  the points where the paths pass within 0.3 m (distance 9 to 879 m, 10 to 16 m/s), gives 0.412 to 0.436 m (88) and
  0.443 to 0.464 m (94).
- It does not follow the car: our rest heights of 12 RSF cars on 129 against the RSF files of maps 129, 429, 450 and
  476 (each map's start line driven by 2 to 10 cars) leave at most 3.3 cm per car after the map's mean is taken off,
  while the cars' `vecCenterOfGravity` heights span 0.28 to 0.38 m (the Golf, 0.28, is 1.8 cm under its map's mean).
  The 2-3 cm left fit the drivers' setups (ride height), which the files do not hold.
- Our ground is the stage's own: the body rests 0.41 m above the `.trk` driveline at the start (the driveline's height
  interpolated at the car), and the car model's tyres put the ground 0.455 m under the body, so RSF's point is 0.83 to
  0.87 m above the road.

So it is neither the centre of gravity nor the model root, and a car-independent height: what RSF's plugin reads
(perhaps a point of its own on the car, or its NGP 7.5's) is not traced; the plugin was not available. The viewer
takes RSF's point as 0.43 m above the body along the body's up (`ghost::RSF_POINT_HEIGHT`, ours: a world-up height
would differ by under 2 cm on these slopes) and places the model root at `-vecCenterOfGravity` from the body.

## What we write

The launcher keeps a finished RSF drive (an RSF car, or any car on an RSF stage) in this layout under its own
`NGP/GhostCars/` (never in RSF's install): the latest always, each best when it beats the stored one or there is none.
Ours: samples every 100 ms of race time from the body pose raised as RSF's point is (above), the race clock's whole
seconds from the countdown's end (penalties not added as they come), the wetness as the middle word, car id 0 and slot
word = the `Cars.ini` index for a game car, zeros in the 16 trailing bytes (RSF's plugin would most likely not read
these files).