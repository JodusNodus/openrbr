"""The original .lsp tokenizer's view of every retail .lsp file, for the reader check in
crates/rbr-formats/tests/lsp_tokens.rs.

Usage: .venv/bin/python -I -B harness/lsp_tokens.py SNAPSHOT OUT  (RBR_DATA: the install tree)

Runs `NTweak::CTokenCursor::Tokenize` (0x52ec80) in the emulator from SNAPSHOT (any; only the code and the CRT's
character tables are used) over each `.lsp` in Root/physics.rbz and Root/misc.rbz, and again over each group token,
as `CBase::Parse` does. Tokens that start in a `;` or `//` comment (outside quotes, to the end of the line) are left
out: the apply walk skips them as unknown keys. OUT gets one line per file, `<archive>!<entry>\t<tree>`, the tree
being the items of the file's outer list: an atom as it is, a quoted token as `"text"`, a group as `(items)`,
separated by single spaces.
"""
import os
import struct
import sys
import zipfile

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402
import rust_step  # noqa: E402

TOKENIZE = 0x52EC80
TEXT_AT = 0x7F000000  # a region no snapshot maps
TEXT_SIZE = 0x100000


def comment_spans(text):
    """[start, end) of each `;` or `//` comment outside quotes."""
    spans, quoted, i = [], False, 0
    while i < len(text):
        c = text[i]
        if c == '"':
            quoted = not quoted
        elif c == "\n":
            quoted = False
        elif not quoted and (c == ";" or text.startswith("//", i)):
            end = text.find("\n", i)
            end = len(text) if end < 0 else end
            spans.append((i, end))
            i = end
            continue
        i += 1
    return spans


class Tokenizer:
    def __init__(self, snapshot):
        self.snap = replay.Snapshot(snapshot)
        self.m = rust_step.machine(self.snap, rust_step.oracle())
        self.m.mu.mem_map(TEXT_AT, TEXT_SIZE)

    def string(self, at):
        out = bytearray()
        while True:
            chunk = bytes(self.m.mu.mem_read(at + len(out), 64))
            n = chunk.find(b"\0")
            out += chunk if n < 0 else chunk[:n]
            if n >= 0:
                return out.decode("latin-1")

    def tokens(self, text_at, cursor):
        """The token pointers Tokenize leaves for the text at text_at (the cursor is 8 bytes of scratch). Its table is
        read before the next call, so every call takes the stub heap from its start."""
        self.m.heap_next = self.m.heap_base
        stop = rust_step.call_from(self.m, self.snap, TOKENIZE, cursor, [text_at, 0])
        if stop:
            raise RuntimeError("Tokenize at %#x: %s" % (text_at, stop))
        table = struct.unpack("<I", self.m.read(cursor, 4))[0]
        out = []
        while True:
            p = struct.unpack("<I", self.m.read(table + 4 * len(out), 4))[0]
            if not p:
                return out
            out.append(p)

    def tree(self, text):
        data = text.encode("latin-1") + b"\0"
        cursor = TEXT_AT + TEXT_SIZE - 0x100
        assert len(data) < cursor - TEXT_AT
        self.m.mu.mem_write(TEXT_AT, data)
        spans = comment_spans(text)

        def in_comment(p):
            off = p - TEXT_AT
            return any(a <= off < b for a, b in spans)

        def items(at, depth):
            out = []
            for p in self.tokens(at, cursor - 8 * depth):
                if in_comment(p):
                    continue
                s = self.string(p)
                if s.startswith("("):
                    out.append("(%s)" % " ".join(items(p, depth + 1)))
                elif s.startswith('"'):
                    out.append(s + '"')
                else:
                    out.append(s)
            return out

        return " ".join(items(TEXT_AT, 0))


def main():
    if len(sys.argv) != 3 or "RBR_DATA" not in os.environ:
        sys.exit(__doc__)
    t = Tokenizer(sys.argv[1])
    lines = []
    for archive in ("Root/physics.rbz", "Root/misc.rbz"):
        path = os.path.join(os.environ["RBR_DATA"], archive)
        with zipfile.ZipFile(path) as z:
            for name in z.namelist():
                if name.endswith(".lsp"):
                    lines.append("%s!%s\t%s" % (archive, name, t.tree(z.read(name).decode("latin-1"))))
    with open(sys.argv[2], "w") as f:
        f.write("\n".join(lines) + "\n")
    print("%d files" % len(lines))


if __name__ == "__main__":
    main()
