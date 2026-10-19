"""diverge.py REF RUN [NOISE...]: first step where RUN departs from REF, ignoring words
that differ anywhere between REF and a NOISE run.
diverge.py --mask REF NOISE...: write REF.mask, the words to ignore that don't depend on the
run (one byte per word), for the plugin's early stop (notes/verification.md). When REF.mask
exists it is ignored too, so a run cut short at its divergence reports the same step.
The mask's noise runs must include count runs (RBR_COUNT=1, our code never runs): any harness
mode shifts the heap, so ~170 words of uninitialised memory and pointers differ from REF for
the whole drive. ref1.bin.mask on cachy = ref2 + refnoise-count1..3. (Until 2026-10-14 every
word that differed at the first common step was ignored instead, which also hid real
differences a transplant makes during the load settle, e.g. BlendB 0x50a650 at car+0x17c.)"""
import os,struct,sys
import numpy as np
def load(path):
    d=open(path,"rb").read(); p=0; steps=[]; offs=[]; n=0
    while p+20<=len(d):
        m,pre,post,sub,cs=struct.unpack_from("<IIIII",d,p); n=0x20+cs+4*0xa00
        if p+20+n>len(d): break
        steps.append(post); offs.append(p+20); p+=20+n
    nw=n//4
    arr=np.empty((len(steps),nw),dtype=np.uint32)
    buf=np.frombuffer(d,dtype=np.uint8)
    for i,o in enumerate(offs): arr[i]=buf[o:o+4*nw].view(np.uint32)
    arr[arr==0x80000000]=0  # -0.0 vs 0.0 is not a divergence
    return dict(zip(steps,range(len(steps)))),arr
def aligned(x,y):
    (ix,ax),(iy,ay)=x,y
    common=sorted(set(ix)&set(iy))
    return common,ax[[ix[s] for s in common]],ay[[iy[s] for s in common]]
def static_mask(ref,a,noises):
    """Words to ignore whatever the run: a is REF's steps to scan for garbage."""
    mask=np.zeros(a.shape[1],dtype=bool)
    # Frame artifacts, not physics: car+0x824 (word 0x844) is set per rendered frame and cleared at
    # its end, so it lives one fast-forward frame longer when transplant/count/shadow overhead packs
    # steps into frames differently; control word 0x18 holds a heap pointer once the controls run out.
    # car+0x658 (bit 1) and car+0x674 (1.0 -> 0.0), words 0x678/0x694, change at a frame-driven event
    # (step 9,729 on tanner-corolla) and shift by whole frames the same way, then agree again.
    mask[[0x844//4, 0x18//4, 0x678//4, 0x694//4]]=True
    # Lane 3 (the unused 4th float) of 16-byte vectors that the original fills from uninitialised
    # stack left by earlier calls (GH2, 2026-10-14: M29E0/M2BC0/Passenger::Step, also Vf6cx/Vf81x/
    # SimEAD0): car+0x8ec, +0x90c, +0x147c, +0x149c, +0x14bc, +0x14cc, +0x14ec, +0x150c. It only feeds the passenger
    # shake payload, not the car's motion; C++ can't reproduce it (user decision: mask it).
    mask[[0x90c//4, 0x92c//4, 0x149c//4, 0x14bc//4, 0x14dc//4, 0x14ec//4, 0x150c//4, 0x152c//4]]=True
    # Same class: car+0x1cc (word 0x1ec), lane 3 of the velocity. On a brake-wall hit Impulse640 adds dir[3]*mag, and
    # dir is Vf6cx's raycast buffer, whose lane 3 RaycastEntry never writes: stale stack (tanner-damage 11820,
    # arctic-snow 12380; lead 2026-10-16). Masking the word still catches any spread into other words.
    mask[0x1ec//4]=True
    # Same for each wheel (frame 0x1720 + k*0xa00; P2W, M3B900): lane 3 at wheel+0x99c and +0x9bc, and
    # the quaternion w at +0x9dc, which E29Quat::Normalize scales from stack garbage. Only lane-wise ops read them
    # (the madd 0x53e423/0x53e67d and MB560's lane-3 copy 0x51b583), so the garbage stays in lane 3; C++ can't reproduce
    # it. Not wheel+0x9ac: M3AFB0 reads it.
    mask[[(0x1720 + k*0xa00 + o)//4 for k in range(4) for o in (0x99c, 0x9bc, 0x9dc)]]=True
    # words that ever hold garbage in the reference (NaN/inf or |x| > 1e20 as float): scratch
    fa=a.view(np.float32)
    with np.errstate(invalid="ignore"):
        mask|=(~np.isfinite(fa)|(np.abs(fa)>1e20)).any(axis=0)
    for noise in noises:
        _,na,nb=aligned(ref,load(noise))
        mask|=(na!=nb).any(axis=0)
    return mask
if __name__=="__main__":  # importable for its load() and static_mask() (harness/rust_step.py)
  if sys.argv[1]=="--mask":
      ref=load(sys.argv[2])
      static_mask(ref,ref[1],sys.argv[3:]).astype(np.uint8).tofile(sys.argv[2]+".mask")
      sys.exit(0)
  ref=load(sys.argv[1]); run=load(sys.argv[2])
  common,a,b=aligned(ref,run)
  nref=len(ref[0])
  if not len(common):  # a run that stalled before its first step: nothing to compare
      print("short run: 0 of %d steps"%nref)
      sys.exit(2)
  mask=static_mask(ref,a,sys.argv[3:])
  if os.path.exists(sys.argv[1]+".mask"):
      mask|=np.fromfile(sys.argv[1]+".mask",dtype=np.uint8).astype(bool)
  diff=(a!=b)&~mask
  rows=np.nonzero(diff.any(axis=1))[0]
  if len(common)<0.95*nref: print("short run: %d of %d steps"%(len(common),nref))
  if len(rows):
      r=rows[0]; ks=np.nonzero(diff[r])[0]
      print("diverged at step %d: %d words, e.g. word offsets %s"%(common[r],len(ks),[hex(4*int(k)) for k in ks[:6]]))
      sys.exit(1)
  print("identical over %d steps (%d words compared)"%(len(common),int((~mask).sum())))
  sys.exit(2 if len(common)<0.95*nref else 0)
