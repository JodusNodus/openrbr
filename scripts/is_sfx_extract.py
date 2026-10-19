"""Extracts files from an InstallShield 2002 self-extracting setup overlay.

Overlay entries are: name\0 relpath\0 version\0 size\0 <size bytes>.
Usage: is_sfx_extract.py <setup.exe> <empty output dir>
(The launcher has its own port for the disc setup: rbr_formats::installshield::sfx_files.)
"""
import os
import struct
import sys


def overlay_offset(d):
    pe = struct.unpack_from('<I', d, 0x3c)[0]
    count = struct.unpack_from('<H', d, pe + 6)[0]
    opt = struct.unpack_from('<H', d, pe + 20)[0]
    return max(sum(struct.unpack_from('<II', d, pe + 24 + opt + i * 40 + 16)[::-1]) for i in range(count))


def extract(src, out):
    """Writes the overlay's files under `out`; returns (size, relpath) per file."""
    out = os.path.abspath(out)
    files = []
    d = open(src, 'rb').read()
    pos = overlay_offset(d)
    while pos < len(d):
        fields = []
        for _ in range(4):
            end = d.index(b'\0', pos)
            fields.append(d[pos:end].decode('latin-1'))
            pos = end + 1
        _, relpath, _, size = fields
        size = int(size)
        dest = os.path.normpath(os.path.join(out, relpath.replace('\\', '/')))
        if not dest.startswith(out + os.sep):
            raise ValueError(f'refusing path outside output dir: {relpath}')
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        with open(dest, 'wb') as f:
            f.write(d[pos:pos + size])
        files.append((size, relpath))
        pos += size
    return files


def main(src, out):
    try:
        files = extract(src, out)
    except ValueError as e:
        sys.exit(str(e))
    for size, relpath in files:
        print(f'{size:>10}  {relpath}')


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
