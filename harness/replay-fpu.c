// x87 results of this CPU (the test machine) for harness/replay.py, which uses them in place of the
// emulator's transcendentals, and harness/replay-fpu.py (the emulator check): reads 24-byte
// cases { u8 op, u8 0, u16 control word, f80 a, f80 b } from stdin and writes
// { f80 r0, f80 r1, u16 status word after the op } for:
// fninit; fldcw cw; fld b; fld a; <op>; fnstsw; fstp r0; fstp r1.
// gcc -O1 -o replay-fpu replay-fpu.c && ./replay-fpu < cases.bin > native.bin
#include <stdio.h>
#include <string.h>
#define RUN(insn) asm volatile("fninit\n fldcw %3\n fldt %4\n fldt %5\n " insn "\n fnstsw %2\n fstpt %0\n fstpt %1" \
    : "=m"(*(char(*)[10])r0), "=m"(*(char(*)[10])r1), "=m"(sw) : "m"(cw), "m"(*(char(*)[10])b), \
      "m"(*(char(*)[10])a))
int main(void)
{
    unsigned char c[24], r0[10], r1[10];
    unsigned short sw;
    while (fread(c, 24, 1, stdin) == 1)
    {
        unsigned short cw;
        unsigned char a[10], b[10];
        memcpy(&cw, c + 2, 2);
        memcpy(a, c + 4, 10);
        memcpy(b, c + 14, 10);
        memset(r0, 0, 10);
        memset(r1, 0, 10);
        switch (c[0])
        {
        case 0: RUN("fadd %%st(1), %%st"); break;
        case 1: RUN("fsub %%st(1), %%st"); break;
        case 2: RUN("fmul %%st(1), %%st"); break;
        case 3: RUN("fdiv %%st(1), %%st"); break;
        case 4: RUN("fsqrt"); break;
        case 5: RUN("fsin"); break;
        case 6: RUN("fcos"); break;
        case 7: RUN("fsincos"); break;
        case 8: RUN("fptan"); break;
        case 9: RUN("fpatan"); break;
        case 10: RUN("fyl2x"); break;
        case 11: RUN("f2xm1"); break;
        case 12: RUN("fscale"); break;
        case 13: RUN("frndint"); break;
        case 14: RUN("fprem"); break;
        }
        fwrite(r0, 10, 1, stdout);
        fwrite(r1, 10, 1, stdout);
        fwrite(&sw, 2, 1, stdout);
    }
    return 0;
}
