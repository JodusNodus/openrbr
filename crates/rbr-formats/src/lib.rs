//! Readers for Richard Burns Rally (2004) data file formats.
//!
//! The game's `.rbz` archives are plain ZIP files (stored + deflate), read by
//! the game through a statically linked minizip `unzip.c`. See
//! `notes/formats.md` "Archives".
//!
//! Car data: `Cars/Cars.ini` (`cars`), the car `.ini` format (`ini`) and its part sections and
//! materials (`car_parts`). See `notes/formats.md` "Cars". Car models (`Cars/<CAR>/<car>.sgc`): `sgc`, see
//! `notes/formats.md` "Car models"; CHELALIC archives and repair data are also covered.
//!
//! Stage readers cover `.mat` material grids, `.fnc` fence groups and texture
//! tables, `.trk` driveline/collision chunks, `.col` collision trees, and
//! `.dls` AnimData tables. `markers` reads the stage's start, split, finish and
//! pace-note keys from the `Driveline` set of the `.dls`. Unknown fields remain
//! raw where the loader does not establish their meaning; see
//! `notes/formats.md` "Stage files", `notes/formats.md` "Collision" and
//! `notes/formats.md` "Markers".
//!
//! Stage geometry `lbs` reads the same chunk framing and parses the render
//! geometry and object lanes; see `notes/formats.md` "Stage geometry". `stage_textures` maps a stage's
//! texture names to archive paths (`TextureFilenameMap.dat`), and `dds` reads the block-compressed
//! textures inside those archives (`notes/formats.md` "Textures and sprites").
//!
//! Animated scenery: `sgs` reads the `SGS` character models (same 3DS Max
//! export framing as `sgc`, with skinned meshes and a LocoMotion node table)
//! and `sga` the `SGA` controller bindings; see `notes/formats.md` "Characters". `lua` parses the character scripts that load
//! and drive them (`notes/renderer.md` "Characters").
//!
//! Sound: `audio` reads `Root/audio.dat` and the audio inis, `eng` the engine sound banks; see
//! `notes/formats.md` "Audio" and `notes/formats.md` "Audio". `fmod` reads the FMOD Studio banks of RSF's cars
//! (`AudioFMOD/<name>.bank`): their recordings and their events' parameter sheets (`notes/formats.md` "Audio").
//!
//! `stage_files` finds a stage's files, retail (`Maps/track-54_E.*`) or in a folder of its own; `rsf` reads the
//! RallySimFans stage folders (`notes/formats.md` "RSF stages").
//!
//! `times` reads `misc/times.ini`, the default reference times of each stage (`notes/formats.md` "Saves and times").
//!
//! `ghost` reads and writes the ghost car files (`.gho`, `.ghc`); see `notes/renderer.md`.
//!
//! `bnk` reads the menu graphics bank (`Generic/PC_Generic_<width>.bnk`): its texture pages and sprites, the stage
//! pictures among them.
//!
//! `strings` reads the game's string tables (`Strings/bstrings.<lang>`), the English texts of the menus.
//!
//! `pe` maps the retail exe's sections, which the physics load reads as the process starts with them.
//!
//! Installing from the disc: `iso9660` reads the disc image, `installshield` the InstallShield cabinets of the disc
//! and the 1.02 patch (and the patch's self-extracting `.exe`).
//!
//! `saves` reads and writes the game's own save files: the encrypted profile (`pf<name>.rbr`) with the championship
//! season inside it, and the records table (`pfresult.rst`); see `notes/formats.md` "Saves and times".
//!
//! `wine` reads a Wine prefix's `user.reg` for the DirectInput instance GUIDs an RSF profile's `.acm` names.

pub mod ai_ini;
pub mod audio;
pub mod bnk;
pub mod car_parts;
pub mod cars;
pub mod col;
pub mod dat;
pub mod data;
pub mod dds;
pub mod dls;
pub mod eng;
pub mod fmod;
pub mod fnc;
pub mod ghost;
pub mod ini;
pub mod installshield;
pub mod iso9660;
pub mod lbs;
pub mod lsp;
pub mod lsp_keys;
pub mod lsp_keys_ngp;
pub mod lsp_tables;
pub mod lua;
pub mod markers;
pub mod mat;
pub mod pe;
pub mod pss;
pub mod rbz;
pub mod repair;
pub mod rsf;
pub mod saves;
pub mod season_dat;
mod sg_framing;
pub mod sga;
pub mod sgc;
pub mod sgs;
pub mod skidmarks;
pub mod stage_files;
pub mod stage_textures;
pub mod strings;
pub mod tga;
pub mod times;
pub mod trk;
pub mod wine;
