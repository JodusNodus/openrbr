"""Put an NGP car into a slot of a physics.rbz, as RSF does (notes/ngp.md, "Test install and recordings"): the slot directory
(e.g. Physics/s_i2003/) is replaced by the car folder's files (common.lsp, surface and r_* files, setups/, ...).

Usage: python3 -I scripts/ngp-install-car.py RBZ SLOT CARDIR
  RBZ     physics.rbz to rewrite in place (keep a copy)
  SLOT    slot directory name, e.g. s_i2003 (car slot 5) or t_coroll (slot 6)
  CARDIR  an NGP car folder, e.g. original/ngp/cars-rsf/Ford_Fiesta_Rally3_ngp6
"""
import os, sys, zipfile


def main(rbz, slot, cardir):
    prefix = "Physics/%s/" % slot
    with zipfile.ZipFile(rbz) as z:
        keep = [(i, z.read(i)) for i in z.infolist() if not i.filename.lower().startswith(prefix.lower())]
    assert len(keep) < len(zipfile.ZipFile(rbz).infolist()), "no %s in %s" % (prefix, rbz)
    tmp = rbz + ".tmp"
    with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as out:
        for info, data in keep:
            out.writestr(info, data)
        out.writestr(prefix, b"")
        for root, dirs, files in os.walk(cardir):
            dirs.sort()
            rel = os.path.relpath(root, cardir).replace(os.sep, "/")
            sub = prefix if rel == "." else prefix + rel + "/"
            if rel != ".":
                out.writestr(sub, b"")
            for f in sorted(files):
                out.write(os.path.join(root, f), sub + f)
    os.replace(tmp, rbz)


if __name__ == "__main__":
    main(*sys.argv[1:4])
