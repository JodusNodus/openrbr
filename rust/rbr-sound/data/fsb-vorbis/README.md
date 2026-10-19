Vorbis setup headers of FMOD's FSB5 encoder, named by the CRC32 an FSB5 sample keeps in their place (chunk 11). These
are the 7 the RSF banks use (`notes/formats/fmod-bank.md`), copied from the `fsbex` crate's lookup table
(`src/encode/vorbis_lookup.rs`, version 0.3.0, https://github.com/astral4/fsbex, MIT OR Apache-2.0,
Copyright (c) 2023 astral4), which takes them from vgmstream.

Files (the only ones taken from fsbex; no code, no FMOD SDK code or binaries): `c4c30a29.bin` 3832, `355295ca.bin`
3189, `38aa59ce.bin` 3832, `6d39bf3e.bin` 3908, `f2781a42.bin` 4140, `84d3ac87.bin` 3189, `2ba1a444.bin` 3460 bytes
(25550 in all). fsbex's license: `LICENSE-MIT` here.
