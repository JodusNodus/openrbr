// cl: /O2 /LD /MD /GR- /D__SUPPORT_PLUGIN__=1 (MSVC 7.1; see harness/README.md)
// rbr-recomp harness plugin. RBR 1.02 loads every DLL in Plugins/ and calls
// RBR_CreatePlugin. We use that hook point to patch the running game in memory;
// the exe on disk stays the untouched reference.
//
// Patches are listed with the original bytes they replace and are only applied
// when every original byte matches (so the NoSSE exe or any other build is left
// alone). Results go to RBRRecompHarness.log in the game folder.
//
// If RBRRecompTransplant.bin (written by harness/transplant.py) is in the game
// folder, its functions, our compiled code rebound to the original's addresses,
// replace the originals: each original entry jumps to a stub that counts the call
// and continues into our copy. RBRRecompCalls.log holds the counts, rewritten every
// 5 s (the game is usually closed without unloading plugins).
//
// If RBRRecompAuto.txt exists ("map car weather tyre", the IRBRGame::StartGame
// arguments), the stage is started from the main loop as soon as the game accepts
// it, and a playback run quits the game once the recorded controls run out, so a
// test needs nobody at the keyboard.
//
// If RBRRecompRecord.on exists, every CPhysics::StepForward call appends the
// player's control and the car's state after the step to RBRRecompRecord-<tick>.bin.
// If RBRRecompPlayback.bin (an earlier recording) exists, its controls drive the car
// instead of the player, and the run is recorded too; harness/compare.py diffs two
// recordings. If RBRRecompEarlyStop.txt names a reference recording, the run quits at
// the first step that departs from it.
// If RBRRecompTrace.on exists (its text: the calls to log per function), the shadow entries
// run the original only and its inputs and outputs go to RBRRecompTrace-<tick>.bin
// (harness/README.md, capture).
#include "IPlugin.h"
#include "IRBRGame.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern "C" __declspec(dllimport) int __stdcall VirtualProtect(void* addr, unsigned long size, unsigned long prot, unsigned long* old);
extern "C" __declspec(dllimport) int __stdcall FlushInstructionCache(void* proc, const void* addr, unsigned long size);
extern "C" __declspec(dllimport) void* __stdcall GetCurrentProcess();
extern "C" __declspec(dllimport) void* __stdcall CreateThread(void* attr, unsigned long stack,
    unsigned long (__stdcall* start)(void*), void* arg, unsigned long flags, unsigned long* id);
extern "C" __declspec(dllimport) void* __stdcall CreateToolhelp32Snapshot(unsigned long flags, unsigned long process);
extern "C" __declspec(dllimport) int __stdcall Thread32First(void* snapshot, void* entry);
extern "C" __declspec(dllimport) int __stdcall Thread32Next(void* snapshot, void* entry);
extern "C" __declspec(dllimport) void __stdcall Sleep(unsigned long ms);
extern "C" __declspec(dllimport) unsigned long __stdcall GetTickCount();
extern "C" __declspec(dllimport) int __stdcall QueryPerformanceCounter(__int64* count);
extern "C" __declspec(dllimport) int __stdcall QueryPerformanceFrequency(__int64* frequency);
extern "C" __declspec(dllimport) void* __stdcall AddVectoredExceptionHandler(unsigned long first, long (__stdcall* handler)(void*));
extern "C" __declspec(dllimport) int __stdcall IsBadReadPtr(const void* p, unsigned long n);
extern "C" __declspec(dllimport) void* __stdcall VirtualAlloc(void* addr, unsigned long size, unsigned long type, unsigned long prot);
extern "C" __declspec(dllimport) unsigned long __stdcall VirtualQuery(const void* addr, void* info, unsigned long size);
extern "C" __declspec(dllimport) unsigned long __stdcall GetCurrentThreadId();
extern "C" __declspec(dllimport) unsigned long __stdcall GetLastError();
extern "C" __declspec(dllimport) void* __stdcall CreateFileA(const char* name, unsigned long access, unsigned long share,
    void* sec, unsigned long disposition, unsigned long flags, void* tmpl);
extern "C" __declspec(dllimport) int __stdcall WriteFile(void* h, const void* buf, unsigned long n, unsigned long* done,
    void* ov);
extern "C" __declspec(dllimport) int __stdcall CloseHandle(void* h);
extern "C" __declspec(dllimport) unsigned long __stdcall GetCurrentDirectoryA(unsigned long n, char* buf);
extern "C" __declspec(dllimport) void* __stdcall GetModuleHandleA(const char* name);
extern "C" __declspec(dllimport) int __stdcall TerminateProcess(void* proc, unsigned code);
extern "C" __declspec(dllimport) int __cdecl wsprintfA(char* buf, const char* fmt, ...);
extern "C" __declspec(dllimport) int __stdcall EnumWindows(int (__stdcall* callback)(void*, long), long param);
extern "C" __declspec(dllimport) unsigned long __stdcall GetCurrentProcessId();
extern "C" __declspec(dllimport) unsigned long __stdcall GetWindowThreadProcessId(void* hwnd, unsigned long* pid);
extern "C" __declspec(dllimport) long __stdcall GetWindowLongA(void* hwnd, int index);
extern "C" __declspec(dllimport) long __stdcall SetWindowLongA(void* hwnd, int index, long value);
extern "C" __declspec(dllimport) long __stdcall CallWindowProcA(void* proc, void* hwnd, unsigned msg, unsigned w, long l);
extern "C" __declspec(dllimport) void* __stdcall OpenThread(unsigned long access, int inherit, unsigned long id);
extern "C" __declspec(dllimport) unsigned long __stdcall SuspendThread(void* thread);
extern "C" __declspec(dllimport) unsigned long __stdcall ResumeThread(void* thread);
extern "C" __declspec(dllimport) int __stdcall GetThreadContext(void* thread, void* context);
extern "C" __declspec(dllimport) int __stdcall SetThreadContext(void* thread, const void* context);
extern "C" __declspec(dllimport) int __stdcall GetExitCodeThread(void* thread, unsigned long* code);

struct Patch
{
    const char* name;
    unsigned addr;
    unsigned char before;
    unsigned char after;
};

// CInputDevice::SetFFBValue 0x004c6b40 sends its constant force with cAxes = 1 and
// Cartesian direction {0}, but CreateFFB 0x004c8c60 created the effect on every
// force-feedback axis the device reports. Windows Logitech drivers report one;
// Wine/SDL report X and Y for the G923, and Wine rejects the mismatched update
// with E_INVALIDARG, so no force ever reaches the wheel. Send both axes, with
// direction (+x, 0): dir[0] takes edx (the device pointer, always > 0) instead
// of esi (0); only the vector's ratio matters.
static const Patch kPatches[] = {
    { "SetFFBValue cAxes 1 -> 2", 0x004c6bc2, 0x01, 0x02 },
    { "SetFFBValue dir[0] esi -> edx", 0x004c6bab, 0x74, 0x54 },
};

static void ApplyPatches(FILE* log)
{
    const unsigned n = sizeof(kPatches) / sizeof(kPatches[0]);
    unsigned i;
    for (i = 0; i < n; i++)
    {
        const unsigned char* p = (const unsigned char*)kPatches[i].addr;
        if (*p != kPatches[i].before)
        {
            fprintf(log, "not applied (found %02x, expected %02x at %08x): this is not the 1.02 SSE exe\n",
                *p, kPatches[i].before, kPatches[i].addr);
            return;
        }
    }
    for (i = 0; i < n; i++)
    {
        unsigned char* p = (unsigned char*)kPatches[i].addr;
        unsigned long old;
        VirtualProtect(p, 1, 0x40 /* PAGE_EXECUTE_READWRITE */, &old);
        *p = kPatches[i].after;
        VirtualProtect(p, 1, old, &old);
        FlushInstructionCache(GetCurrentProcess(), p, 1);
        fprintf(log, "applied %s at %08x\n", kPatches[i].name, kPatches[i].addr);
    }
}

static void WriteCode(unsigned char* p, const void* src, unsigned n)
{
    unsigned long old;
    VirtualProtect(p, n, 0x40 /* PAGE_EXECUTE_READWRITE */, &old);
    memcpy(p, src, n);
    VirtualProtect(p, n, old, &old);
    FlushInstructionCache(GetCurrentProcess(), p, n);
}

struct TransplantFunc
{
    unsigned addr;
    unsigned offset;
    unsigned char check[8];
    unsigned flags;       // bit 0: shadow-eligible leaf; bit 1: no copy of ours (count runs only)
    unsigned trampoline;  // blob offset of the original's displaced prologue + jmp back
    unsigned retKind;     // 0 void, 1 eax, 2 st0, 3 edx:eax, 4 unknown (eax, flagged)
    // Bytes [lo, hi) around an input (source: pushad register order, then 8 + stack
    // argument) that either copy may write: all a shadow call snapshots and restores.
    // nspec ~0 (version 1 and 2 files): [-0x40, 0x800) around every input.
    unsigned nspec;
    struct
    {
        unsigned source;
        short lo, hi;
    } spec[16];
};
typedef char TransplantFuncIs160Bytes[sizeof(TransplantFunc) == 160 ? 1 : -1];  // ShadowEntry indexes them

struct TransplantFix
{
    unsigned kind;
    unsigned offset;
    unsigned value;
};

static unsigned s_count;
static volatile int s_shadowIndex = -1, s_shadowPhase;  // last shadowed call, for crash logs
static volatile unsigned long s_shadowThread;          // the thread running it
static unsigned char* s_blob;
static unsigned s_blobSize;
static const TransplantFunc* s_funcs;
static volatile long* s_calls;

static unsigned long __stdcall LogCalls(void*);

static void Rel32(unsigned char* at, unsigned target)
{
    *(unsigned*)at = target - ((unsigned)at + 4);
}

// First access violation / illegal instruction: log where, and which transplanted
// function it is in (the one whose copy starts closest below EIP), then let the
// game's own handling run.
// " (our copy of <original> +off)" when addr is inside the blob; offsets of copied EH
// pieces and constants attribute to the function copied just before them.
static char* BlobAddress(char* out, unsigned addr)
{
    *out = 0;
    if (addr - (unsigned)s_blob >= s_blobSize)
        return out;
    unsigned best = 0;
    for (unsigned i = 0; i < s_count; i++)
        if (!(s_funcs[i].flags & 2) && (unsigned)s_blob + s_funcs[i].offset <= addr
            && (!best || s_funcs[i].offset > s_funcs[best - 1].offset))
            best = i + 1;
    if (best)
        out += wsprintfA(out, " (our copy of %08x +%x)", s_funcs[best - 1].addr,
            addr - ((unsigned)s_blob + s_funcs[best - 1].offset));
    return out;
}

// The game folder as the plugin loads. The game moves the current directory into Maps\ while it
// loads a stage, so the files written after load are named through GameFile.
static char s_gameDir[260];

static const char* GameFile(char* out, const char* name)
{
    wsprintfA(out, "%s\\%s", s_gameDir, name);
    return out;
}

// Appends to RBRRecompHarness.log with Win32 calls only: the fail paths run while some
// other thread may hold the CRT's file or heap lock forever.
static void AppendLog(const char* text, unsigned n)
{
    char path[320];
    void* h = CreateFileA(GameFile(path, "RBRRecompHarness.log"), 4 /* FILE_APPEND_DATA */, 3 /* share read/write */, 0,
        4 /* OPEN_ALWAYS */, 0x80 /* normal */, 0);
    if (h == (void*)-1)
        return;
    unsigned long done;
    WriteFile(h, text, n, &done, 0);
    CloseHandle(h);
}

// A stack overflow seen by OnException: its eip and the stack above it, for the watchdog to log
// (Wine ends the thread quietly if the handler itself runs out of stack).
static volatile unsigned s_overflowEip, s_overflowEsp, s_overflowStack[2048];
static const unsigned kWatchLookahead = 0x01660b74, kWatchPaceSpeed = 0x01660b80;
static const unsigned kWatchMarkerTick = 0x004783b0;
static volatile int s_writeWatch, s_watchArmed, s_watchStageStarted;
static volatile unsigned s_watchFrame, s_watchThreadId;
// Word watches. Default: the marker probe above (Dr0, Dr1; Dr2 the marker tick). With
// RBRRecompWriteWatch.on listing up to 4 hex addresses (one per word), those words are watched
// instead (Dr0-Dr3, no marker tick), and an optional "steps N" line quits at step N. Each hit
// logs whether the write came inside StepForward (s_stepDepth > 0) and the step counter.
static unsigned s_watchAddr[4] = { kWatchLookahead, kWatchPaceSpeed };
static unsigned s_watchWords[4], s_watchSeen[4];
static unsigned s_watchCount = 2, s_watchListed, s_watchSteps, s_watchCar;
static volatile int s_stepDepth;
static void* s_stepSelf;
static int s_exceptionHandlerAdded;
static void* s_mainThread;
static unsigned long s_mainThreadId;
static long __stdcall OnException(void* info);

static void EnsureExceptionHandler()
{
    if (!s_exceptionHandlerAdded)
    {
        AddVectoredExceptionHandler(1, OnException);
        s_exceptionHandlerAdded = 1;
    }
}

static char* CodeWords(char* p, const unsigned* words, unsigned n, unsigned max);
static unsigned StepCounter(void* physics);

struct ThreadEntry32
{
    unsigned long size, usage, id, owner;
    long basePriority, deltaPriority;
    unsigned long flags;
};

static int ArmWriteWatchThread(void* thread)
{
    unsigned ctx[0x2cc / 4];
    memset(ctx, 0, sizeof(ctx));
    ctx[0] = 0x00010010;  // CONTEXT_DEBUG_REGISTERS
    if (SuspendThread(thread) == (unsigned long)-1)
        return 0;
    int ok = GetThreadContext(thread, ctx);
    if (ok)
    {
        ctx[0] = 0x00010010;
        ctx[5] = 0;                // Dr6
        if (s_watchListed)
        {
            ctx[6] = 0;
            for (unsigned i = 0; i < s_watchCount; i++)  // Dr0-Dr3: local, write, 4 bytes
            {
                ctx[1 + i] = s_watchAddr[i];
                ctx[6] |= (1u << (2 * i)) | (1u << (16 + 4 * i)) | (3u << (18 + 4 * i));
            }
        }
        else
        {
            ctx[1] = kWatchLookahead;  // Dr0: write, 4 bytes
            ctx[2] = kWatchPaceSpeed;  // Dr1: write, 4 bytes
            ctx[3] = kWatchMarkerTick; // Dr2: before marker ticker
            ctx[6] = 0x00dd0015;       // Dr7: local, 4-byte write watches and exec bp
        }
        ok = SetThreadContext(thread, ctx);
    }
    ResumeThread(thread);
    return ok;
}

static unsigned long __stdcall ArmWriteWatchThreads(void*)
{
    void* snapshot = CreateToolhelp32Snapshot(4 /* TH32CS_SNAPTHREAD */, 0);
    if (snapshot == (void*)-1)
    {
        static const char message[] = "write-watch: thread snapshot failed\n";
        AppendLog(message, sizeof(message) - 1);
        return 0;
    }
    ThreadEntry32 entry;
    memset(&entry, 0, sizeof(entry));
    entry.size = sizeof(entry);
    unsigned armed = 0;
    unsigned long pid = GetCurrentProcessId(), self = GetCurrentThreadId();
    if (Thread32First(snapshot, &entry))
    {
        do
        {
            if (entry.owner == pid && entry.id != self)
            {
                void* thread = OpenThread(0x005a /* QUERY | GET/SET_CONTEXT | SUSPEND */, 0, entry.id);
                if (thread)
                {
                    if (ArmWriteWatchThread(thread))
                    {
                        char line[96];
                        armed++;
                        wsprintfA(line, "write-watch armed thread %08x\n", entry.id);
                        AppendLog(line, (unsigned)strlen(line));
                    }
                    CloseHandle(thread);
                }
            }
            entry.size = sizeof(entry);
        } while (Thread32Next(snapshot, &entry));
    }
    CloseHandle(snapshot);
    char line[96];
    wsprintfA(line, "write-watch: armed %u existing threads before auto-start\n", armed);
    AppendLog(line, (unsigned)strlen(line));
    return 0;
}

static void WatchWrite(unsigned index, unsigned eip, unsigned esp)
{
    static char buf[1024];
    unsigned addr = s_watchAddr[index];
    unsigned value = *(volatile unsigned*)addr;
    unsigned old = s_watchWords[index];
    s_watchWords[index] = value;
    char* p = buf;
    p += wsprintfA(p, "write-watch frame %u %s step %u word %08x old %08x new %08x eip %08x stack",
        s_watchFrame, s_stepDepth ? "in-stepforward" : "frame-loop", s_stepSelf ? StepCounter(s_stepSelf) : 0,
        addr, s_watchSeen[index] ? old : 0, value, eip);
    s_watchSeen[index] = 1;
    p = CodeWords(p, (const unsigned*)esp, 64, 16);
    p += wsprintfA(p, "\n");
    AppendLog(buf, p - buf);
}

static void WatchMarkerTick(unsigned eip, unsigned esp)
{
    static const unsigned frames[] = { 0, 1, 300, 1000, 2000, 5000 };
    static char buf[1024];
    int wanted = 0;
    for (unsigned i = 0; i < sizeof(frames) / sizeof(frames[0]); i++)
        if (s_watchFrame == frames[i])
            wanted = 1;
    if (!wanted)
        return;
    char* p = buf;
    p += wsprintfA(p, "write-watch before marker tick frame %u word %08x=%08x word %08x=%08x eip %08x stack",
        s_watchFrame, kWatchLookahead, *(volatile unsigned*)kWatchLookahead,
        kWatchPaceSpeed, *(volatile unsigned*)kWatchPaceSpeed, eip);
    p = CodeWords(p, (const unsigned*)esp, 64, 16);
    p += wsprintfA(p, "\n");
    AppendLog(buf, p - buf);
}

static long __stdcall OnException(void* info)
{
    unsigned* rec = *(unsigned**)info;               // EXCEPTION_RECORD
    unsigned char* ctx = ((unsigned char**)info)[1];  // CONTEXT (x86)
    if (rec[0] == 0x80000004 && s_writeWatch)
    {
        unsigned status = *(unsigned*)(ctx + 0x14);  // CONTEXT.Dr6
        unsigned eip = *(unsigned*)(ctx + 0xb8), esp = *(unsigned*)(ctx + 0xc4);
        if (s_watchListed)
        {
            // Under Wine the exception context's Dr6 stays clear (the debug registers do
            // take: the readback at arming shows them), so each listed word is checked for a
            // change instead. A write of the value already there is not seen.
            for (unsigned i = 0; i < s_watchCount; i++)
                if (!IsBadReadPtr((void*)s_watchAddr[i], 4)
                    && (!s_watchSeen[i] || *(volatile unsigned*)s_watchAddr[i] != s_watchWords[i]))
                    WatchWrite(i, eip, esp);
        }
        else
        {
            if (!(status & 7))
                return 0;
            static int firstWatchHit;
            if (!firstWatchHit++)
            {
                char line[128];
                wsprintfA(line, "write-watch debug exception tid %08x dr6 %08x eip %08x\n",
                    GetCurrentThreadId(), status, eip);
                AppendLog(line, (unsigned)strlen(line));
            }
            if (status & 1)
                WatchWrite(0, eip, esp);
            if (status & 2)
                WatchWrite(1, eip, esp);
            if ((status & 4) && eip == kWatchMarkerTick)
                WatchMarkerTick(eip, esp);
        }
        *(unsigned*)(ctx + 0x14) = 0;  // clear the debug status before continuing
        return -1;                     // EXCEPTION_CONTINUE_EXECUTION
    }
    static int logged;  // how many logged (the first few: later ones may follow from the first)
    if (s_shadowPhase == 2 && GetCurrentThreadId() == s_shadowThread)
        return 0;  // our copy faulting under SafeInvoke (counted there)
    if (rec[0] == 0xC00000FD && !s_overflowEsp)  // stack overflow: no room to log here
    {
        s_overflowEip = *(unsigned*)(ctx + 0xb8);
        s_overflowEsp = *(unsigned*)(ctx + 0xc4);
        unsigned n = 0;
        while (n < sizeof(s_overflowStack) / 4 && !IsBadReadPtr((unsigned*)s_overflowEsp + n, 4))
            n++;
        memcpy((void*)s_overflowStack, (void*)s_overflowEsp, n * 4);
        return 0;
    }
    if (logged >= 6 || rec[0] < 0xC0000000)  // hardware faults (AV, stack overflow, illegal instruction, ...)
        return 0;
    logged++;
    unsigned eip = *(unsigned*)(ctx + 0xb8), esp = *(unsigned*)(ctx + 0xc4);
    static char buf[2048];  // static: little stack left after a fault deep in a recursion
    char* p = buf;
    p += wsprintfA(p, "exception %08x at %08x (access %08x)", rec[0], eip, rec[0] == 0xC0000005 ? rec[6] : 0);
    p = BlobAddress(p, eip);
    if (s_shadowIndex >= 0)
        p += wsprintfA(p, "\n  last shadowed call: %08x, phase %d (1 original, 2 ours, 3 compare/restore, 0 done)",
            s_funcs[s_shadowIndex].addr, s_shadowPhase);
    p += wsprintfA(p, "\n  stack:");
    for (int k = 0; k < 24; k++)
    {
        unsigned v = ((unsigned*)esp)[k];
        p += wsprintfA(p, " %08x", v);
        p = BlobAddress(p, v);  // return addresses into our copies
    }
    p += wsprintfA(p, "\n");
    AppendLog(buf, p - buf);
    return 0;
}

// ---- shadow comparison ----
// A shadowed function runs twice on the same input: the original (through its
// trampoline) and our copy. Registers, the 64 bytes of stack arguments and every
// memory region a register or argument points at are snapshotted first and restored
// between the runs; afterwards the return value, x87 st0, callee-saved registers,
// bytes popped and the regions are compared, and the game continues with the
// original's results. Only leaves (no calls, no globals) are shadowed, so running
// twice has no side effects outside those regions.
struct Regs
{
    unsigned edi, esi, ebp, esp, ebx, edx, ecx, eax;  // pushad order
};

struct ShadowFrame
{
    Regs regs;
    unsigned index;
    unsigned ret;
    unsigned args[16];
};

struct ShadowStats
{
    unsigned calls;
    unsigned mismatches;
    unsigned fpuInput;  // calls that arrived with x87 input and ran the original only
    char first[160];
    unsigned in[7];     // eax, ecx, edx and the first 4 stack arguments of that call
};

struct Region
{
    unsigned char* at;
    unsigned size;
    unsigned base;   // the pointer value it was found through
    const char* from;
    unsigned snap;   // offset of the snapshot in s_snap
};

static ShadowStats* s_shadow;
static Region s_regions[16];
static unsigned s_nregions;
static unsigned char s_snap[16 * 0x880], s_after[16 * 0x880];

// Invoke() parameters and results (the physics thread is the only caller).
static Regs s_invIn, s_invOut;
static unsigned s_invArgs[16];
static unsigned s_invTarget, s_invSaveEsp, s_invEspAfter, s_invHasSt0;
static double s_invSt0;
static unsigned char s_fenv[28];
// SSE registers: LTCG callers keep values in the xmm registers a callee leaves alone, so
// each run starts from the caller's xmm0-7 and the caller gets the original's back.
static unsigned char s_xmmIn[128], s_invXmm[128], s_xmmRet[128];
// ShadowEntry's way out
static unsigned s_retN, s_retAddr, s_pushSt0;
static double s_st0;

static __declspec(naked) void Invoke()
{
    __asm
    {
        pushad
        mov s_invSaveEsp, esp
        sub esp, 64
        mov ecx, 16
        lea esi, s_invArgs
        mov edi, esp
        rep movsd
        movups xmm0, s_xmmIn
        movups xmm1, s_xmmIn + 16
        movups xmm2, s_xmmIn + 32
        movups xmm3, s_xmmIn + 48
        movups xmm4, s_xmmIn + 64
        movups xmm5, s_xmmIn + 80
        movups xmm6, s_xmmIn + 96
        movups xmm7, s_xmmIn + 112
        mov ebx, s_invIn.ebx
        mov esi, s_invIn.esi
        mov edi, s_invIn.edi
        mov ebp, s_invIn.ebp
        mov ecx, s_invIn.ecx
        mov edx, s_invIn.edx
        mov eax, s_invIn.eax
        call dword ptr [s_invTarget]
        mov s_invOut.eax, eax
        mov s_invOut.ebx, ebx
        mov s_invOut.ecx, ecx
        mov s_invOut.edx, edx
        mov s_invOut.esi, esi
        mov s_invOut.edi, edi
        mov s_invOut.ebp, ebp
        mov s_invEspAfter, esp
        movups s_invXmm, xmm0
        movups s_invXmm + 16, xmm1
        movups s_invXmm + 32, xmm2
        movups s_invXmm + 48, xmm3
        movups s_invXmm + 64, xmm4
        movups s_invXmm + 80, xmm5
        movups s_invXmm + 96, xmm6
        movups s_invXmm + 112, xmm7
        fnstenv s_fenv
        fldcw word ptr s_fenv       // fnstenv masked all exceptions; restore the control word
        movzx eax, word ptr s_fenv + 4
        shr eax, 11
        and eax, 7                  // TOP
        lea ecx, [eax + eax]
        movzx edx, word ptr s_fenv + 8
        shr edx, cl
        and edx, 3                  // tag of st0
        mov s_invHasSt0, 0
        cmp edx, 3
        je empty
        fstp s_invSt0
        mov s_invHasSt0, 1
    empty:
        mov esp, s_invSaveEsp
        popad
        ret
    }
}


struct MemInfo  // MEMORY_BASIC_INFORMATION
{
    unsigned base, allocBase, allocProtect, size, state, protect, type;
};

// Whether the region holding `at` is committed, writable, non-guard memory. Asked, not
// probed: IsBadWritePtr writes to the pages (Wine's does `*p |= 0`), racing other threads.
static int WritableRegion(unsigned at, MemInfo& mi)
{
    return VirtualQuery((void*)at, &mi, sizeof(mi)) && mi.state == 0x1000 /* MEM_COMMIT */
        && (mi.protect & 0xcc) /* (EXECUTE_)READWRITE / WRITECOPY */ && !(mi.protect & 0x100) /* PAGE_GUARD */;
}

// The end of the writable memory from `at`, at most `to`.
static unsigned WritableEnd(unsigned at, unsigned to)
{
    MemInfo mi;
    while (at < to && WritableRegion(at, mi))
        at = mi.base + mi.size;
    return at < to ? at : to;
}

// Snapshot [v + from, v + to) when v is a pointer the function could write through.
static void AddRegion(unsigned v, const char* name, int from, int to, unsigned argsAt, unsigned stackTop,
    unsigned stackLimit)
{
    if (s_nregions == 16 || v < 0x10000 || v - (unsigned)s_blob < s_blobSize)
        return;
    if (v >= stackLimit && v < argsAt)
        return;  // below the caller's arguments: the stub's and our own frames
    unsigned lo = v + from, hi = v + to;
    if (hi - lo > 0x880)
        hi = lo + 0x880;
    if (v >= argsAt && v < stackTop)
    {
        if (lo < argsAt)
            lo = argsAt;  // never below the caller's arguments: that's our own frames
        if (hi > stackTop)
            hi = stackTop;
    }
    else
    {
        unsigned at = lo > v ? lo : v;  // the pointer, or the first byte written past it
        MemInfo mi;
        if (!WritableRegion(at, mi))
            return;
        if (lo < mi.base)
            lo = mi.base;
        hi = WritableEnd(at, hi);
    }
    if (hi <= lo || hi - lo > 0x880)
        return;
    Region& r = s_regions[s_nregions];
    r.at = (unsigned char*)lo;
    r.size = hi - lo;
    r.base = v;
    r.from = name;
    r.snap = s_nregions * 0x880;
    memcpy(s_snap + r.snap, r.at, r.size);
    s_nregions++;
}

extern "C" void* __cdecl _exception_info();  // excpt.h's GetExceptionInformation()
static unsigned s_faultCode, s_faultAt;

static int FaultFilter(void* info)
{
    unsigned* rec = *(unsigned**)info;
    s_faultCode = rec[0];
    s_faultAt = rec[3];
    return 1;  // EXCEPTION_EXECUTE_HANDLER
}

// Invoke() under SEH: 0 if the callee faulted (the x87 stack is reset then).
static int SafeInvoke()
{
    __try
    {
        Invoke();
    }
    __except (FaultFilter(_exception_info()))
    {
        unsigned short cw = *(unsigned short*)s_fenv;
        __asm
        {
            fnstcw cw
            fninit
            fldcw cw
        }
        return 0;
    }
    return 1;
}

static unsigned s_callMismatched;

static const unsigned* s_callIn;  // the call's eax, ecx, edx, then its stack arguments

static void Mismatch(ShadowStats& st, const char* what, unsigned a, unsigned b)
{
    if (s_callMismatched++)
        return;  // count calls, not differing words
    if (st.mismatches++ == 0)
    {
        _snprintf(st.first, sizeof(st.first) - 1, "%s: %08x vs %08x (as float %g vs %g)", what, a, b,
            (double)*(float*)&a, (double)*(float*)&b);
        memcpy(st.in, s_callIn, sizeof(st.in));
    }
}

// Snapshot the memory around a call's inputs: the transplant specs, or the default window.
static void SnapshotInputs(const ShadowFrame* f, const TransplantFunc& fn, int chaseState = 0)
{
    unsigned argsAt = (unsigned)f->args, stackTop, stackLimit;
    __asm
    {
        mov eax, fs:[4]
        mov stackTop, eax
        mov eax, fs:[8]
        mov stackLimit, eax
    }
    s_nregions = 0;
    static const char* names[] = { "edi", "esi", "ebp", "esp", "ebx", "edx", "ecx", "eax" };
    static const char* argn[] = { "arg0", "arg1", "arg2", "arg3", "arg4", "arg5", "arg6", "arg7" };
    unsigned k;
    if (fn.nspec > 16)
    {
        for (k = 0; k < 8; k++)
            if (k != 3)
                AddRegion((&f->regs.edi)[k], names[k], -0x40, 0x800, argsAt, stackTop, stackLimit);
        for (k = 0; k < 8; k++)
            AddRegion(f->args[k], argn[k], -0x40, 0x800, argsAt, stackTop, stackLimit);
    }
    for (k = 0; fn.nspec <= 16 && k < fn.nspec; k++)
    {
        unsigned src = fn.spec[k].source & 15;
        AddRegion(src < 8 ? (&f->regs.edi)[src] : f->args[src - 8], src < 8 ? names[src] : argn[src - 8],
            fn.spec[k].lo, fn.spec[k].hi, argsAt, stackTop, stackLimit);
    }
    if (chaseState)
    {
        // one hop through this+0x10 (CScratch's tyre parameters) and this+0x14 (m_state): the
        // tyre methods read their sub-objects there
        static const unsigned hops[] = { 0x10, 0x14 };
        static const char* hopNames[] = { "+0x10", "+0x14" };
        for (k = 0; k < 2; k++)
        {
            MemInfo mi;
            if (WritableRegion(f->regs.ecx + hops[k], mi))
                AddRegion(*(unsigned*)(f->regs.ecx + hops[k]), hopNames[k], -0x40, 0x800, argsAt, stackTop, stackLimit);
        }
    }
}

// ---- capture (RBRRecompTrace.on) ----
// Shadow entries run only the original (no copy of ours). The first s_traceMax calls of each
// function are written to RBRRecompTrace-<tick>.bin: the registers and first 16 stack arguments
// on entry, the x87 control word and whether an x87 value came in, the registers, popped bytes,
// st0 and control word on return, and each input region's bytes before and after the call.
// Layout (u32 little-endian unless noted): header 'RBTR', 1, calls per function; then per call
// 'CAL1', addr, index, cw_in, fpu_in, eax ebx ecx edx esi edi ebp, args[16], eax ebx ecx edx esi
// edi ebp, popped, has_st0, st0 (f64), cw_out, nregions; per region base, at, size, name length,
// name bytes, pre bytes, post bytes.
static FILE* s_trace;
static unsigned s_traceMax;
static int s_capture;

static void TraceWord(unsigned v)
{
    fwrite(&v, 4, 1, s_trace);
}

static void TraceRecord(const TransplantFunc& fn, const ShadowFrame* f, unsigned index, unsigned cwIn,
    unsigned fpuIn, const Regs& a, unsigned popped, unsigned hasSt0, double st0, unsigned cwOut)
{
    TraceWord(0x314c4143);  // 'CAL1'
    TraceWord(fn.addr);
    TraceWord(index);
    TraceWord(cwIn);
    TraceWord(fpuIn);
    TraceWord(f->regs.eax); TraceWord(f->regs.ebx); TraceWord(f->regs.ecx); TraceWord(f->regs.edx);
    TraceWord(f->regs.esi); TraceWord(f->regs.edi); TraceWord(f->regs.ebp);
    fwrite(f->args, 4, 16, s_trace);
    TraceWord(a.eax); TraceWord(a.ebx); TraceWord(a.ecx); TraceWord(a.edx);
    TraceWord(a.esi); TraceWord(a.edi); TraceWord(a.ebp);
    TraceWord(popped);
    TraceWord(hasSt0);
    fwrite(&st0, 8, 1, s_trace);
    TraceWord(cwOut);
    TraceWord(s_nregions);
    for (unsigned k = 0; k < s_nregions; k++)
    {
        const Region& r = s_regions[k];
        TraceWord(r.base);
        TraceWord((unsigned)r.at);
        TraceWord(r.size);
        TraceWord((unsigned)strlen(r.from));
        fwrite(r.from, 1, strlen(r.from), s_trace);
        fwrite(s_snap + r.snap, 1, r.size, s_trace);
        fwrite(s_after + r.snap, 1, r.size, s_trace);
    }
    fflush(s_trace);
}

static void __cdecl CaptureC(ShadowFrame* f)
{
    const TransplantFunc& fn = s_funcs[f->index];
    ShadowStats& st = s_shadow[f->index];
    unsigned index = st.calls++;
    unsigned short fpuTag;
    __asm
    {
        fnstenv s_fenv
        fldcw word ptr s_fenv
        mov ax, word ptr s_fenv + 8
        mov fpuTag, ax
    }
    unsigned cwIn = *(unsigned short*)s_fenv;
    unsigned fpuIn = fpuTag != 0xffff;
    unsigned logged = index < s_traceMax;
    if (logged)
    {
        // the default window around every input: a port reads what it doesn't write
        TransplantFunc wide = fn;
        wide.nspec = ~0u;
        SnapshotInputs(f, wide, 1);
    }
    unsigned args[16];
    memcpy(args, f->args, sizeof(args));
    s_invIn = f->regs;
    memcpy(s_invArgs, args, sizeof(args));
    s_invTarget = (unsigned)s_blob + fn.trampoline;
    Invoke();
    Regs a = s_invOut;
    unsigned popped = s_invEspAfter - (s_invSaveEsp - 64);
    unsigned hasSt0 = s_invHasSt0;
    double st0 = s_invSt0;
    unsigned cwOut = *(unsigned short*)s_fenv;
    memcpy(s_xmmRet, s_invXmm, sizeof(s_xmmRet));
    if (logged)
    {
        for (unsigned k = 0; k < s_nregions; k++)
            memcpy(s_after + s_regions[k].snap, s_regions[k].at, s_regions[k].size);
        TraceRecord(fn, f, index, cwIn, fpuIn, a, popped, hasSt0, st0, cwOut);
    }
    // the original's writes stay: nothing is restored
    f->regs.eax = a.eax; f->regs.ebx = a.ebx; f->regs.ecx = a.ecx; f->regs.edx = a.edx;
    f->regs.esi = a.esi; f->regs.edi = a.edi; f->regs.ebp = a.ebp;
    s_retN = popped;
    s_pushSt0 = hasSt0;
    s_st0 = st0;
}

static void __cdecl ShadowC(ShadowFrame* f)
{
    if (s_capture)
    {
        CaptureC(f);
        return;
    }
    const TransplantFunc& fn = s_funcs[f->index];
    ShadowStats& st = s_shadow[f->index];
    st.calls++;
    s_callMismatched = 0;
    s_shadowIndex = f->index;
    s_shadowThread = GetCurrentThreadId();
    s_shadowPhase = 1;
    unsigned short fpuTag;
    __asm
    {
        fnstenv s_fenv
        fldcw word ptr s_fenv
        mov ax, word ptr s_fenv + 8
        mov fpuTag, ax
    }
    if (fpuTag != 0xffff)
    {
        // x87 input (an LTCG st0 hand-off): can't be replayed twice; run the original only
        st.fpuInput++;
        s_invIn = f->regs;
        memcpy(s_invArgs, f->args, sizeof(s_invArgs));
        s_invTarget = (unsigned)s_blob + fn.trampoline;
        Invoke();
        f->regs.eax = s_invOut.eax; f->regs.ebx = s_invOut.ebx; f->regs.ecx = s_invOut.ecx;
        f->regs.edx = s_invOut.edx; f->regs.esi = s_invOut.esi; f->regs.edi = s_invOut.edi;
        f->regs.ebp = s_invOut.ebp;
        s_retN = s_invEspAfter - (s_invSaveEsp - 64);
        memcpy(s_xmmRet, s_invXmm, sizeof(s_xmmRet));
        s_pushSt0 = s_invHasSt0;
        s_st0 = s_invSt0;
        s_shadowPhase = 0;
        return;
    }
    SnapshotInputs(f, fn);
    unsigned k;
    unsigned args[16];
    memcpy(args, f->args, sizeof(args));
    unsigned callIn[7] = { f->regs.eax, f->regs.ecx, f->regs.edx, args[0], args[1], args[2], args[3] };
    s_callIn = callIn;

    // A: the original
    s_invIn = f->regs;
    memcpy(s_invArgs, args, sizeof(args));
    s_invTarget = (unsigned)s_blob + fn.trampoline;
    Invoke();
    Regs a = s_invOut;
    unsigned aPopped = s_invEspAfter - (s_invSaveEsp - 64), aHas = s_invHasSt0;
    double aSt0 = s_invSt0;
    memcpy(s_xmmRet, s_invXmm, sizeof(s_xmmRet));
    // capture every region before restoring any: regions can overlap
    for (k = 0; k < s_nregions; k++)
        memcpy(s_after + s_regions[k].snap, s_regions[k].at, s_regions[k].size);
    for (k = 0; k < s_nregions; k++)
        memcpy(s_regions[k].at, s_snap + s_regions[k].snap, s_regions[k].size);
    memcpy(f->args, args, sizeof(args));

    // B: ours
    s_shadowPhase = 2;
    s_invIn = f->regs;
    memcpy(s_invArgs, args, sizeof(args));
    s_invTarget = (unsigned)s_blob + fn.offset;
    if (!SafeInvoke())
    {
        // our copy faulted: count it, put the original's results back and carry on
        Mismatch(st, "ours crashed (exception code, address)", s_faultCode, s_faultAt);
        for (k = 0; k < s_nregions; k++)
            memcpy(s_regions[k].at, s_after + s_regions[k].snap, s_regions[k].size);
        f->regs.eax = a.eax; f->regs.ebx = a.ebx; f->regs.ecx = a.ecx; f->regs.edx = a.edx;
        f->regs.esi = a.esi; f->regs.edi = a.edi; f->regs.ebp = a.ebp;
        s_retN = aPopped;
        s_pushSt0 = aHas;
        s_st0 = aSt0;
        s_shadowPhase = 0;
        return;
    }
    Regs& b = s_invOut;
    s_shadowPhase = 3;
    unsigned bPopped = s_invEspAfter - (s_invSaveEsp - 64);

    if (aPopped != bPopped)
        Mismatch(st, "bytes popped", aPopped, bPopped);
    if ((fn.retKind == 1 || fn.retKind == 3) && a.eax != b.eax)
        Mismatch(st, "eax", a.eax, b.eax);
    if (fn.retKind == 4 && a.eax != b.eax)
        Mismatch(st, "eax (return type unknown, may be void)", a.eax, b.eax);
    if (fn.retKind == 3 && a.edx != b.edx)
        Mismatch(st, "edx", a.edx, b.edx);
    if (aHas != s_invHasSt0)
        Mismatch(st, "st0 present", aHas, s_invHasSt0);
    else if (aHas && memcmp(&aSt0, &s_invSt0, 8) != 0)
    {
        float fa = (float)aSt0, fb = (float)s_invSt0;
        Mismatch(st, "st0", *(unsigned*)&fa, *(unsigned*)&fb);
    }
    if (a.ebx != b.ebx || a.esi != b.esi || a.edi != b.edi || a.ebp != b.ebp)
        Mismatch(st, "callee-saved ebx", a.ebx, b.ebx);
    for (k = 0; k < s_nregions; k++)
    {
        const Region& r = s_regions[k];
        const unsigned char* pa = s_after + r.snap;
        for (unsigned o = 0; o + 4 <= r.size; o += 4)
            if (*(unsigned*)(pa + o) != *(unsigned*)(r.at + o))
            {
                char what[48];
                _snprintf(what, sizeof(what) - 1, "memory [%s%+d]", r.from, (int)((unsigned)r.at + o - r.base));
                what[sizeof(what) - 1] = 0;
                Mismatch(st, what, *(unsigned*)(pa + o), *(unsigned*)(r.at + o));
                break;
            }
    }

    // continue with the original's results
    for (k = 0; k < s_nregions; k++)
        memcpy(s_regions[k].at, s_after + s_regions[k].snap, s_regions[k].size);
    f->regs.eax = a.eax;
    f->regs.ebx = a.ebx;
    f->regs.ecx = a.ecx;
    f->regs.edx = a.edx;
    f->regs.esi = a.esi;
    f->regs.edi = a.edi;
    f->regs.ebp = a.ebp;
    s_retN = aPopped;
    s_pushSt0 = aHas;
    s_st0 = aSt0;
    s_shadowPhase = 0;
}

static volatile int s_inShadow;

static __declspec(naked) void ShadowEntry()
{
    __asm
    {
        push eax
        mov eax, 1
        xchg eax, s_inShadow            // atomic: two threads can arrive at once
        test eax, eax
        pop eax
        jz outer
        // nested (another thread, or a callee re-entering): run the original directly
        push eax
        mov eax, [esp + 4]              // the stub's index
        shl eax, 2
        add eax, s_calls
        lock inc dword ptr [eax]        // shadow runs count these in RBRRecompCalls.log
        mov eax, [esp + 4]
        imul eax, eax, 160              // sizeof(TransplantFunc)
        add eax, s_funcs
        mov eax, [eax + 20]             // trampoline offset
        add eax, s_blob
        mov [esp + 4], eax
        pop eax
        ret                             // to the trampoline; the caller's return address stays
    outer:
        movups s_xmmIn, xmm0
        movups s_xmmIn + 16, xmm1
        movups s_xmmIn + 32, xmm2
        movups s_xmmIn + 48, xmm3
        movups s_xmmIn + 64, xmm4
        movups s_xmmIn + 80, xmm5
        movups s_xmmIn + 96, xmm6
        movups s_xmmIn + 112, xmm7
        pushad
        mov eax, esp
        push eax
        call ShadowC
        add esp, 4
        movups xmm0, s_xmmRet
        movups xmm1, s_xmmRet + 16
        movups xmm2, s_xmmRet + 32
        movups xmm3, s_xmmRet + 48
        movups xmm4, s_xmmRet + 64
        movups xmm5, s_xmmRet + 80
        movups xmm6, s_xmmRet + 96
        movups xmm7, s_xmmRet + 112
        mov s_inShadow, 0
        popad
        add esp, 4                  // the stub's pushed index
        cmp s_pushSt0, 0
        je noflt
        fld s_st0
    noflt:
        pop s_retAddr
        add esp, s_retN
        jmp dword ptr [s_retAddr]
    }
}

// ---- load-time snapshots (RBRRecompSnapAt.txt) ----
// Each line of RBRRecompSnapAt.txt is `<address hex> <calls> [arg-offset hex min hex max hex]`:
// when the transplant patches that function (its entry, in whatever mode the run uses), its first
// <calls> calls matching the optional 32-bit value range at `[arg0 + offset]` are dumped:
// RBRRecompSnapAt-<address>-<k>.bin at entry (version 2 header: the entry address as the step,
// the stack from the entry esp - kSnapBelow to + kSnapAbove, the registers as the call sees them)
// and RBRRecompSnapAt-<address>-<k>-after.bin on return (no stack). The return address is
// replaced while the call runs and put back before the function returns, so the stack keeps
// the value the original leaves. The dump saves and restores the stack below the call, the
// registers and the x87/SSE state, as SnapStepForward does, so the run stays the original's.
static const unsigned kSnapAtMax = 32, kSnapAtDepth = 16;
static unsigned s_snapAtAddr[kSnapAtMax], s_snapAtLimit[kSnapAtMax], s_snapAtSeen[kSnapAtMax], s_snapAtCount;
static unsigned s_snapAtArgOffset[kSnapAtMax], s_snapAtMin[kSnapAtMax], s_snapAtMaxValue[kSnapAtMax];
static volatile unsigned s_snapAtIdx;                       // the list entry whose entry stub runs
static unsigned s_snapAtRetAddr[kSnapAtDepth], s_snapAtRetOrd[kSnapAtDepth],
    s_snapAtRetIdx[kSnapAtDepth], s_snapAtDepth;
static unsigned s_snapAtPatchSlot, s_snapAtHook, s_snapAtJump, s_snapAtTarget;  // handed between C and the asm
static unsigned s_snapAtRegs[8];       // the registers at the stub, pushad order: edi esi ebp esp ebx edx ecx eax
static unsigned char s_snapAtFlags[2];  // lahf's AH and the overflow flag (seto)
static __declspec(align(16)) unsigned char s_snapAtFx[512];
static unsigned SnapAtEntryAddress();  // the naked stubs, below
static unsigned SnapAtRetAddress();
static void EnsureSnapBuffers();

static int SnapAtFind(unsigned addr)
{
    for (unsigned k = 0; k < s_snapAtCount; k++)
        if (s_snapAtAddr[k] == addr)
            return (int)k;
    return -1;
}

static void LoadSnapAt(FILE* log)
{
    FILE* f = fopen("RBRRecompSnapAt.txt", "r");
    if (!f)
        return;
    char line[256];
    while (s_snapAtCount < kSnapAtMax && fgets(line, sizeof(line), f))
    {
        unsigned addr, limit, offset, min, max;
        int fields = sscanf(line, "%x %u %x %x %x", &addr, &limit, &offset, &min, &max);
        if (fields < 2)
            continue;
        unsigned k = s_snapAtCount++;
        s_snapAtAddr[k] = addr, s_snapAtLimit[k] = limit;
        if (fields == 5)
            s_snapAtArgOffset[k] = offset, s_snapAtMin[k] = min, s_snapAtMaxValue[k] = max;
    }
    fclose(f);
    s_snapAtHook = SnapAtRetAddress();
    fprintf(log, "snapat: %u functions listed for load-time dumps\n", s_snapAtCount);
}

static void ApplyTransplant(FILE* log)
{
    FILE* f = fopen("RBRRecompTransplant.bin", "rb");
    if (!f)
        return;
    unsigned head[5];
    if (fread(head, 4, 5, f) != 5 || head[0] != 0x54524252 /* 'RBRT' */ || head[1] < 1 || head[1] > 3)
    {
        fprintf(log, "transplant: bad header\n");
        fclose(f);
        return;
    }
    const unsigned nfunc = head[2], nfix = head[3], size = head[4];
    TransplantFunc* funcs = new TransplantFunc[nfunc];
    TransplantFix* fixes = new TransplantFix[nfix];
    // blob, then one 12-byte stub per function, then the call counters
    unsigned char* mem = (unsigned char*)VirtualAlloc(0, size + 16 * nfunc + 4 * nfunc + 32 * nfunc + 16,
        0x3000 /* MEM_COMMIT | MEM_RESERVE */, 0x40 /* PAGE_EXECUTE_READWRITE */);
    // version 1 entries stop after the check bytes (no shadow data), version 2 after retKind
    const unsigned entry = head[1] == 1 ? 16 : head[1] == 2 ? 28 : sizeof(TransplantFunc);
    unsigned i;
    memset(funcs, 0, nfunc * sizeof(*funcs));
    for (i = 0; mem && i < nfunc; i++)
    {
        if (fread(&funcs[i], entry, 1, f) != 1)
            break;
        if (head[1] < 3)
            funcs[i].nspec = ~0u;
    }
    if (!mem || i != nfunc || fread(fixes, sizeof(*fixes), nfix, f) != nfix
        || fread(mem, 1, size, f) != size)
    {
        fprintf(log, "transplant: truncated file\n");
        fclose(f);
        return;
    }
    fclose(f);
    for (i = 0; i < nfunc; i++)
    {
        if (memcmp((const void*)funcs[i].addr, funcs[i].check, 8) != 0)
        {
            fprintf(log, "transplant: original bytes at %08x differ, nothing applied\n", funcs[i].addr);
            return;
        }
    }
    for (i = 0; i < nfix; i++)
    {
        unsigned char* at = mem + fixes[i].offset;
        if (fixes[i].kind == 0)
            *(unsigned*)at += (unsigned)mem;
        else if (fixes[i].kind == 1)
            Rel32(at, fixes[i].value);
        else
            Rel32(at, (unsigned)mem + fixes[i].value);
    }
    unsigned char* stubs = mem + ((size + 15) & ~15u);
    volatile long* calls = (volatile long*)(stubs + 16 * nfunc);
    unsigned char* snapStubs = (unsigned char*)(calls + nfunc);  // 32 bytes each, see SnapAtEntry
    LoadSnapAt(log);
    if (s_snapAtCount)
        EnsureSnapBuffers();
    FILE* on = fopen("RBRRecompShadow.on", "rb");
    if (on)
        fclose(on);
    // count run: every function with a trampoline is counted and runs the original
    FILE* count = fopen("RBRRecompCount.on", "rb");
    if (count)
    {
        fclose(count);
        on = 0;
    }
    // capture run (RBRRecompTrace.on, the calls to log per function): shadow entries, original only
    FILE* trace = count ? 0 : fopen("RBRRecompTrace.on", "rb");
    if (trace)
    {
        char text[16] = { 0 };
        fread(text, 1, sizeof(text) - 1, trace);
        fclose(trace);
        s_traceMax = atoi(text) > 0 ? atoi(text) : 64;
        char name[64];
        sprintf(name, "RBRRecompTrace-%lu.bin", GetTickCount());
        s_trace = fopen(name, "wb");
        if (s_trace)
        {
            unsigned head[3] = { 0x52544252 /* 'RBTR' */, 1, s_traceMax };
            fwrite(head, 4, 3, s_trace);
            fflush(s_trace);
            s_capture = 1;
            fprintf(log, "capture: writing %s, the first %u calls of each eligible function\n", name, s_traceMax);
        }
    }
    // before any entry is patched: other threads may call in at once
    s_count = nfunc;
    s_funcs = funcs;
    s_calls = calls;
    s_blob = mem;
    s_blobSize = size;
    s_shadow = new ShadowStats[nfunc];
    memset(s_shadow, 0, nfunc * sizeof(ShadowStats));
    unsigned patched = 0;
    for (i = 0; i < nfunc; i++)
    {
        unsigned char* stub = stubs + 16 * i;
        if (funcs[i].addr == 0x004804f0 || funcs[i].addr == 0x0048cd90)
        {
            // CApplication::Tic and the control Emit carry our own hooks (auto start, recorder)
            fprintf(log, "transplant: %08x left alone (hooked by the harness)\n", funcs[i].addr);
            continue;
        }
        if (count ? !funcs[i].trampoline : (funcs[i].flags & 2) || (on || s_capture) && !(funcs[i].flags & 1))
            continue;  // left original: shadow runs leave what can't be shadowed, count runs what can't be counted
        if (count)
        {
            stub[0] = 0xF0; stub[1] = 0xFF; stub[2] = 0x05;  // lock inc dword [calls + 4i]
            *(unsigned*)(stub + 3) = (unsigned)(calls + i);
            stub[7] = 0xE9;                                   // jmp the original's trampoline
            Rel32(stub + 8, (unsigned)mem + funcs[i].trampoline);
            patched++;
        }
        else if ((on || s_capture) && (funcs[i].flags & 1))
        {
            stub[0] = 0x68;                               // push i
            *(unsigned*)(stub + 1) = i;
            stub[5] = 0xE9;                               // jmp ShadowEntry
            Rel32(stub + 6, (unsigned)&ShadowEntry);
            patched++;
        }
        else
        {
            stub[0] = 0xF0; stub[1] = 0xFF; stub[2] = 0x05;  // lock inc dword [calls + 4i]
            *(unsigned*)(stub + 3) = (unsigned)(calls + i);
            stub[7] = 0xE9;                                   // jmp our copy
            Rel32(stub + 8, (unsigned)mem + funcs[i].offset);
        }
        unsigned char* entry = stub;
        int k = SnapAtFind(funcs[i].addr);
        if (k >= 0)
        {
            // mov [s_snapAtIdx], k; mov [s_snapAtTarget], stub; jmp SnapAtEntry (no push: esp stays the call's)
            unsigned char* snap = snapStubs + 32 * i;
            snap[0] = 0xC7; snap[1] = 0x05;
            *(unsigned*)(snap + 2) = (unsigned)&s_snapAtIdx;
            *(unsigned*)(snap + 6) = (unsigned)k;
            snap[10] = 0xC7; snap[11] = 0x05;
            *(unsigned*)(snap + 12) = (unsigned)&s_snapAtTarget;
            *(unsigned*)(snap + 16) = (unsigned)stub;
            snap[20] = 0xE9;
            Rel32(snap + 21, SnapAtEntryAddress());
            entry = snap;
        }
        unsigned char jmp[5] = { 0xE9 };                  // original entry: jmp stub (or the snapshot stub)
        *(unsigned*)(jmp + 1) = (unsigned)entry - (funcs[i].addr + 5);
        WriteCode((unsigned char*)funcs[i].addr, jmp, 5);
    }
    fprintf(log, "transplant: %u functions, %u %s (%u fixups, %u bytes)\n", nfunc,
        on || s_capture || count ? patched : nfunc,
        count ? "counted, all original" : s_capture ? "captured, original only, the rest left original"
        : on ? "shadowed, the rest left original" : "replaced", nfix, size);
    if (count && patched < nfunc)  // built without transplant.py --count: no trampolines, so no counts
        fprintf(log, "transplant: WARNING %u of %u functions have no trampoline and are NOT counted (their 0 in "
            "RBRRecompCalls.log means not measured; build the file with transplant.py --count)\n", nfunc - patched, nfunc);
    unsigned long id;
    CreateThread(0, 0, LogCalls, 0, 0, &id);
    EnsureExceptionHandler();
}

static unsigned long __stdcall LogCalls(void*)
{
    char path[320];
    for (;;)
    {
        Sleep(5000);
        FILE* log = fopen(GameFile(path, "RBRRecompCalls.log"), "w");
        if (!log)
            continue;
        for (unsigned i = 0; i < s_count; i++)
            fprintf(log, "%08x %ld\n", s_funcs[i].addr, s_calls[i]);
        fclose(log);
        log = fopen(GameFile(path, "RBRRecompShadow.log"), "w");
        if (!log)
            continue;
        for (unsigned j = 0; j < s_count; j++)
            if (s_shadow[j].calls)
            {
                const ShadowStats& st = s_shadow[j];
                fprintf(log, "%08x calls %u mismatches %u x87-input %u", s_funcs[j].addr, st.calls, st.mismatches,
                    st.fpuInput);
                if (st.mismatches)
                    fprintf(log, "  first: %s  in: eax %08x ecx %08x edx %08x args %08x %08x %08x %08x", st.first,
                        st.in[0], st.in[1], st.in[2], st.in[3], st.in[4], st.in[5], st.in[6]);
                fprintf(log, "\n");
            }
        fclose(log);
    }
}

// ---- recorder ----
// CPhysics vtable 0x0073f98c, slot 2 StepForward(unsigned substeps) 0x005273a0: one
// call per frame. Each substep reads PhysicsEngine.MaxTimeStep at definition +0x1608c
// (0.0070 s from the retail Root/physics.rbz). CPhysics+0x20 is its CState, whose
// step counter (CUnsigned value) is at +0x18. 0x008ef660 holds the player CCar*;
// CCar+0x470 holds its 4 CWheel*.
//
// The player's controls reach the car through CMultiSignal1<CSigCarControl>::Emit
// 0x0048cd90 (0x20-byte payload), called from the quantizer at 0x0048d24e. The
// recorder keeps the last payload and stores it with the step it fed. In playback
// (RBRRecompPlayback.bin, an earlier recording) the live player emits are dropped and
// the recorded control is emitted before each step instead, one substep per
// StepForward call, so the physics sees the same input sequence as the recording.
static const unsigned kStepSlot = 0x0073f994, kStepForward = 0x005273a0;
static const unsigned kEmit = 0x0048cd90;  // player call returns to 0x0048d253; prologue resumes at 0x0048cd96
static const unsigned kCarPtr = 0x008ef660;
static const unsigned kCarBytes = 0x1700, kWheelBytes = 0xa00, kControlBytes = 0x20;
// Playback physics steps per requested step: 8, or the count in RBRRecompFastForward.txt (1 plays at the game's own
// pace, one step per step the frame asks for, as a live drive steps).
static unsigned s_fastForward = 8;
static const unsigned char kEmitPrologue[6] = { 0x57, 0x8b, 0xf9, 0x8b, 0x47, 0x04 };

// Optional RBRRecompPeek.txt probes. A chain starts with *address (read a
// pointer at an absolute address), then adds offsets and may use bare * to
// read another pointer. The final address is read using the declared type.
enum PeekType { PEEK_F32 = 1, PEEK_U32 = 2, PEEK_U8 = 3, PEEK_I32 = 4 };
enum PeekOpKind { PEEK_ADD, PEEK_DEREF, PEEK_ABS_DEREF };
struct PeekOp
{
    unsigned kind;
    unsigned value;
};
struct PeekProbe
{
    char name[64];
    unsigned type;
    unsigned opCount;
    PeekOp ops[16];
};
static PeekProbe s_peeks[64];
static unsigned s_peekCount;
static FILE* s_peek;

static int PeekNumber(const char* text, unsigned* value)
{
    unsigned base = 10, n = 0;
    const char* p = text;
    if (p[0] == '0' && (p[1] == 'x' || p[1] == 'X'))
    {
        base = 16;
        p += 2;
    }
    if (!*p)
        return 0;
    for (; *p; p++)
    {
        unsigned digit;
        if (*p >= '0' && *p <= '9')
            digit = *p - '0';
        else if (base == 16 && *p >= 'a' && *p <= 'f')
            digit = *p - 'a' + 10;
        else if (base == 16 && *p >= 'A' && *p <= 'F')
            digit = *p - 'A' + 10;
        else
            return 0;
        if (digit >= base || n > (0xffffffffu - digit) / base)
            return 0;
        n = n * base + digit;
    }
    *value = n;
    return 1;
}

static unsigned PeekTypeOf(const char* text)
{
    if (strcmp(text, "f32") == 0) return PEEK_F32;
    if (strcmp(text, "u32") == 0) return PEEK_U32;
    if (strcmp(text, "u8") == 0) return PEEK_U8;
    if (strcmp(text, "i32") == 0) return PEEK_I32;
    return 0;
}

static int ParsePeekLine(char* line, PeekProbe* probe)
{
    char* tokens[32];
    unsigned n = 0;
    char* token = strtok(line, " \t\r\n");
    while (token && n < sizeof(tokens) / sizeof(tokens[0]))
    {
        tokens[n++] = token;
        token = strtok(0, " \t\r\n");
    }
    if (n < 3 || token || strlen(tokens[0]) >= sizeof(probe->name))
        return 0;
    probe->type = PeekTypeOf(tokens[n - 1]);
    if (!probe->type)
        return 0;
    strcpy(probe->name, tokens[0]);
    probe->opCount = 0;
    for (unsigned i = 1; i + 1 < n; i++)
    {
        PeekOp op;
        const char* number;
        if (tokens[i][0] == '*' && tokens[i][1])
        {
            op.kind = PEEK_ABS_DEREF;
            number = tokens[i] + 1;
        }
        else if (strcmp(tokens[i], "*") == 0)
        {
            op.kind = PEEK_DEREF;
            number = 0;
        }
        else if (tokens[i][0] == '+' && tokens[i][1])
        {
            op.kind = PEEK_ADD;
            number = tokens[i] + 1;
        }
        else
            return 0;
        if (probe->opCount >= sizeof(probe->ops) / sizeof(probe->ops[0]))
            return 0;
        if (number && !PeekNumber(number, &op.value))
            return 0;
        probe->ops[probe->opCount++] = op;
    }
    return probe->opCount != 0 && probe->ops[0].kind == PEEK_ABS_DEREF;
}

static void LoadPeeks(FILE* log)
{
    void* exists = CreateFileA("RBRRecompPeek.txt", 0x80000000 /* GENERIC_READ */, 1 /* FILE_SHARE_READ */,
        0, 3 /* OPEN_EXISTING */, 0x80 /* normal */, 0);
    if (exists == (void*)-1)
        return;
    CloseHandle(exists);
    FILE* f = fopen("RBRRecompPeek.txt", "r");
    if (!f)
        return;
    char line[512];
    unsigned lineNo = 0, bad = 0;
    while (fgets(line, sizeof(line), f))
    {
        lineNo++;
        char* p = line;
        while (*p == ' ' || *p == '\t') p++;
        if (!*p || *p == '\r' || *p == '\n' || *p == '#')
            continue;
        if (s_peekCount == sizeof(s_peeks) / sizeof(s_peeks[0])
            || !ParsePeekLine(line, &s_peeks[s_peekCount]))
        {
            fprintf(log, "peek: invalid line %u\n", lineNo);
            bad++;
            continue;
        }
        s_peekCount++;
    }
    fclose(f);
    if (!s_peekCount)
    {
        fprintf(log, "peek: no valid probes (%u invalid lines)\n", bad);
        return;
    }
    char name[64];
    sprintf(name, "RBRRecompPeek-%lu.bin", GetTickCount());
    s_peek = fopen(name, "wb");
    if (!s_peek)
    {
        fprintf(log, "peek: could not open %s\n", name);
        s_peekCount = 0;
        return;
    }
    unsigned header[3] = { 0x314b4550 /* 'PEK1' */, 1, s_peekCount };
    fwrite(header, sizeof(header), 1, s_peek);
    for (unsigned i = 0; i < s_peekCount; i++)
    {
        unsigned char type = (unsigned char)s_peeks[i].type;
        unsigned short len = (unsigned short)strlen(s_peeks[i].name);
        fwrite(&type, 1, 1, s_peek);
        fwrite(&len, 2, 1, s_peek);
        fwrite(s_peeks[i].name, 1, len, s_peek);
    }
    fprintf(log, "peek: writing %s (%u probes, %u invalid lines)\n", name, s_peekCount, bad);
}

static int PeekDeref(unsigned at, unsigned* value)
{
    if (!at || IsBadReadPtr((const void*)at, 4))
        return 0;
    *value = *(unsigned*)at;
    return *value != 0;
}

static int PeekAddress(const PeekProbe* probe, unsigned* address)
{
    unsigned current = 0;
    for (unsigned i = 0; i < probe->opCount; i++)
    {
        const PeekOp& op = probe->ops[i];
        if (op.kind == PEEK_ADD)
        {
            unsigned next = current + op.value;
            if (next < current)
                return 0;
            current = next;
        }
        else
        {
            unsigned at = op.kind == PEEK_ABS_DEREF ? op.value : current;
            if (!PeekDeref(at, &current))
                return 0;
        }
    }
    *address = current;
    return current != 0;
}

static void PeekRecord(unsigned step)
{
    if (!s_peek)
        return;
    fwrite(&step, 4, 1, s_peek);
    for (unsigned i = 0; i < s_peekCount; i++)
    {
        const PeekProbe& probe = s_peeks[i];
        unsigned address = 0, value = 0;
        int valid = PeekAddress(&probe, &address);
        unsigned size = probe.type == PEEK_U8 ? 1 : 4;
        if (valid && IsBadReadPtr((const void*)address, size))
            valid = 0;
        if (!valid)
            value = probe.type == PEEK_F32 ? 0x7fc00000u
                : probe.type == PEEK_I32 ? 0x80000000u : 0xffffffffu;
        else if (probe.type == PEEK_U8)
            value = *(unsigned char*)address;
        else
            value = *(unsigned*)address;
        fwrite(&value, size, 1, s_peek);
    }
}

static void ClosePeek()
{
    if (s_peek)
    {
        fflush(s_peek);
        fclose(s_peek);
        s_peek = 0;
    }
}

struct Control
{
    unsigned step;
    unsigned char data[kControlBytes];
};

static FILE* s_rec;
static volatile unsigned s_lastStepTick;  // for the watchdog (failure handling below)
static unsigned char s_lastControl[kControlBytes];
static void* s_emitThis;
static Control* s_play;
static unsigned s_playCount, s_playPos;

static unsigned StepCounter(void* physics)
{
    return *(unsigned*)(*(unsigned char**)((unsigned char*)physics + 0x20) + 0x18);
}

// ---- early stop ----
// RBRRecompEarlyStop.txt holds the path of a reference recording of the same playback,
// with <path>.mask next to it (harness/diverge.py --mask: one byte per word, nonzero to
// ignore). Each recorded step is compared with the reference step of the same counter as
// diverge.py does (masked words and -0.0 vs 0.0 are ignored); at the first divergence the
// run quits, so the recording ends
// there and diverge.py reports the same step. Runs that match go to the end.
static const unsigned kFrameWords = (kControlBytes + kCarBytes + 4 * kWheelBytes) / 4;
static FILE* s_ref;
static unsigned* s_refSteps;  // post counter of each reference record, increasing
static unsigned s_refCount, s_refPos, s_lastPost;
static unsigned char s_ignore[kFrameWords];
static unsigned s_frame[kFrameWords], s_refFrame[kFrameWords];

static void LoadEarlyStop(FILE* log)
{
    FILE* f = fopen("RBRRecompEarlyStop.txt", "r");
    if (!f)
        return;
    char path[260], mask[270];
    int got = fgets(path, sizeof path, f) != 0;
    fclose(f);
    if (!got)
        return;
    path[strcspn(path, "\r\n")] = 0;
    sprintf(mask, "%s.mask", path);
    FILE* m = fopen(mask, "rb");
    if (!m || fread(s_ignore, 1, kFrameWords, m) != kFrameWords)
    {
        fprintf(log, "early stop: no mask %s, off\n", mask);
        if (m)
            fclose(m);
        return;
    }
    fclose(m);
    FILE* r = fopen(path, "rb");
    if (!r)
    {
        fprintf(log, "early stop: no reference %s, off\n", path);
        return;
    }
    unsigned cap = 1 << 14, head[5];
    s_refSteps = new unsigned[cap];
    while (fread(head, 4, 5, r) == 5 && head[0] == 0x314d5246 && head[4] == kCarBytes)
    {
        if (s_refCount == cap)
        {
            unsigned* more = new unsigned[cap * 2];
            memcpy(more, s_refSteps, cap * sizeof(unsigned));
            delete[] s_refSteps;
            s_refSteps = more;
            cap *= 2;
        }
        s_refSteps[s_refCount++] = head[2];
        fseek(r, kFrameWords * 4, SEEK_CUR);
    }
    s_ref = r;
    fprintf(log, "early stop: %u reference steps from %s\n", s_refCount, path);
}

static void Quit()
{
    if (s_rec)
        fclose(s_rec);
    ClosePeek();
    TerminateProcess(GetCurrentProcess(), 0);  // ExitProcess can block in DLL detach under Wine
}

static void CompareStep(unsigned post)
{
    if (post < s_lastPost)  // the counter restarted (another stage): nothing to compare with
    {
        fclose(s_ref);
        s_ref = 0;
        return;
    }
    s_lastPost = post;
    while (s_refPos < s_refCount && s_refSteps[s_refPos] < post)
        s_refPos++;
    if (s_refPos == s_refCount || s_refSteps[s_refPos] != post)
        return;
    fseek(s_ref, s_refPos * (20 + kFrameWords * 4) + 20, SEEK_SET);
    if (fread(s_refFrame, 4, kFrameWords, s_ref) != kFrameWords)
        return;
    unsigned differ = 0, first = 0;
    for (unsigned i = 0; i < kFrameWords; i++)
    {
        unsigned a = s_frame[i] == 0x80000000 ? 0 : s_frame[i], b = s_refFrame[i] == 0x80000000 ? 0 : s_refFrame[i];
        if (a == b || s_ignore[i])
            continue;
        if (!differ++)
            first = i;
    }
    if (!differ)
        return;
    char buf[128];
    int n = wsprintfA(buf, "early stop: diverged at step %u (%u words, first at offset %x)\n", post, differ, first * 4);
    AppendLog(buf, n);
    Quit();
}

static void Record(unsigned pre, unsigned post, unsigned substeps, const unsigned char* control)
{
    unsigned char* car = *(unsigned char**)kCarPtr;
    if (!car || IsBadReadPtr(car, kCarBytes))
        return;
    unsigned head[5] = { 0x314d5246 /* 'FRM1' */, pre, post, substeps, kCarBytes };
    unsigned char* at = (unsigned char*)s_frame;
    memcpy(at, control, kControlBytes);
    memcpy(at + kControlBytes, car, kCarBytes);
    for (int i = 0; i < 4; i++)
    {
        unsigned char* w = ((unsigned char**)(car + 0x470))[i];
        unsigned char* to = at + kControlBytes + kCarBytes + i * kWheelBytes;
        if (w && !IsBadReadPtr(w, kWheelBytes))
            memcpy(to, w, kWheelBytes);
        else
            memset(to, 0, kWheelBytes);
    }
    fwrite(head, 4, 5, s_rec);
    fwrite(s_frame, 4, kFrameWords, s_rec);
    if (s_ref)
        CompareStep(post);
}

// SnapStepForward when snapshots are on (LoadSnapshots). The hook's frame must stay as it is
// without the switch (its size sets the step's esp, so the stack garbage it sees), hence asm.
static unsigned s_snapTarget;

static void CallStepForward(void* self, unsigned substeps)
{
    unsigned target = kStepForward;
    __asm
    {
        push substeps
        mov ecx, self
        mov eax, target
        cmp s_snapTarget, 0
        je call_it
        mov eax, s_snapTarget
    call_it:
        call eax
    }
}

// With NGP the player's controls are taken at the car's control input instead (HookCarControl).
// CarControlResume is that function past the hook: the displaced prologue, then the rest.
static const unsigned kCarControl = 0x004e91f0;  // CCar control input, thiscall(payload)
static const unsigned char kCarControlPrologue[6] = { 0x0f, 0x57, 0xc0, 0x83, 0xec, 0x28 };
static int s_carControlHooked;
// NGP.dll's image: only its calls of the control input are the player's controls (the car's own
// calls, UpdateMrr's at the setup, pass through).
static unsigned s_ngpLo, s_ngpHi;

static __declspec(naked) void CarControlResume()
{
    __asm
    {
        xorps xmm0, xmm0
        sub esp, 0x28
        push 0x004e91f6
        ret
    }
}

static void CallEmit(void* self, const void* sig)
{
    // through our hook (the return address isn't the player's), or past the car hook
    unsigned target = s_carControlHooked ? (unsigned)&CarControlResume : kEmit;
    __asm
    {
        push sig
        mov ecx, self
        call target
    }
}

// ---- allocation log (RBRRecompAllocLog.on) ----
// The exe's heap calls go through its MSVCR71 imports (operator new 0x0070bec2 is linked in
// and calls malloc; operator delete is imported). With RBRRecompAllocLog.on, those import
// slots point at the Log* hooks below, which call the CRT and append one record per call to a
// VirtualAlloc'd buffer: { kind, thread id, return address, a, b, result }. Kinds: 1 operator
// new (return address: new's caller), 2 malloc, 3 calloc (a count, b size), 4 realloc (a size,
// b the old block), 5 _strdup (a the length with its NUL), 6 operator delete, 7 operator
// delete[], 8 free (a the block); 9 a dump (a its version, b its step or entry address, and in
// the return address word its substeps or ordinal), logged as SnapClose closes it, so the log can
// be cut where snap-0 was taken.
// A thread appends the complete records to RBRRecompAllocs.bin ('RBRA', 1, then the records)
// every second. The heap's placement differs from run to run; its order is what the log
// gives, for the Rust load path's placement (lead/reports/rust-load-path.md, H3).
extern "C" __declspec(dllimport) long __stdcall InterlockedIncrement(volatile long* p);
extern "C" void* _ReturnAddress(void);
#pragma intrinsic(_ReturnAddress)
static const unsigned kAllocMax = 4 << 20, kNewMallocRet = 0x0070bee0;
struct AllocRecord
{
    unsigned kind;  // set last: 0 while the record is being filled
    unsigned thread, ret, a, b, result;
};
static AllocRecord* s_allocs;
static volatile long s_allocN;
typedef void* (__cdecl* MallocFn)(unsigned);
typedef void* (__cdecl* CallocFn)(unsigned, unsigned);
typedef void* (__cdecl* ReallocFn)(void*, unsigned);
typedef char* (__cdecl* StrdupFn)(const char*);
typedef void (__cdecl* FreeFn)(void*);
static MallocFn s_malloc;
static CallocFn s_calloc;
static ReallocFn s_realloc;
static StrdupFn s_strdup;
static FreeFn s_delete, s_deleteArray, s_free;

static void AllocLog(unsigned kind, unsigned ret, unsigned a, unsigned b, unsigned result)
{
    unsigned i = (unsigned)InterlockedIncrement(&s_allocN) - 1;
    if (i >= kAllocMax)
        return;
    volatile AllocRecord* r = &s_allocs[i];  // volatile: the stores stay in order, kind last
    r->thread = GetCurrentThreadId(), r->ret = ret, r->a = a, r->b = b, r->result = result;
    r->kind = kind;
}

static void* __cdecl LogMalloc(unsigned n)
{
    void* p = s_malloc(n);
    unsigned ret = (unsigned)_ReturnAddress();
    if (ret == kNewMallocRet)  // operator new: push esi (size), and new's own push esi, sit below its return address
        AllocLog(1, (&n)[2], n, 0, (unsigned)p);
    else
        AllocLog(2, ret, n, 0, (unsigned)p);
    return p;
}

static void* __cdecl LogCalloc(unsigned count, unsigned size)
{
    void* p = s_calloc(count, size);
    AllocLog(3, (unsigned)_ReturnAddress(), count, size, (unsigned)p);
    return p;
}

static void* __cdecl LogRealloc(void* old, unsigned n)
{
    void* p = s_realloc(old, n);
    AllocLog(4, (unsigned)_ReturnAddress(), n, (unsigned)old, (unsigned)p);
    return p;
}

static char* __cdecl LogStrdup(const char* s)
{
    char* p = s_strdup(s);
    AllocLog(5, (unsigned)_ReturnAddress(), p ? (unsigned)strlen(p) + 1 : 0, 0, (unsigned)p);
    return p;
}

static void __cdecl LogDelete(void* p)
{
    AllocLog(6, (unsigned)_ReturnAddress(), (unsigned)p, 0, 0);
    s_delete(p);
}

static void __cdecl LogDeleteArray(void* p)
{
    AllocLog(7, (unsigned)_ReturnAddress(), (unsigned)p, 0, 0);
    s_deleteArray(p);
}

static void __cdecl LogFree(void* p)
{
    AllocLog(8, (unsigned)_ReturnAddress(), (unsigned)p, 0, 0);
    s_free(p);
}

static unsigned long __stdcall WriteAllocs(void*)
{
    char path[320];
    void* h = CreateFileA(GameFile(path, "RBRRecompAllocs.bin"), 0x40000000 /* GENERIC_WRITE */, 1 /* share read */, 0,
        2 /* CREATE_ALWAYS */, 0x80, 0);
    if (h == (void*)-1)
        return 0;
    unsigned head[2] = { 0x41524252 /* 'RBRA' */, 1 }, done = 0;
    unsigned long wrote;
    WriteFile(h, head, sizeof(head), &wrote, 0);
    for (;;)
    {
        Sleep(1000);
        unsigned n = (unsigned)s_allocN < kAllocMax ? (unsigned)s_allocN : kAllocMax, end = done;
        while (end < n && ((volatile AllocRecord*)s_allocs)[end].kind)
            end++;
        if (end > done)
            WriteFile(h, &s_allocs[done], (end - done) * sizeof(AllocRecord), &wrote, 0);
        done = end;
    }
}

// The exe's import slot of MSVCR71's `name`: the slot's address, or 0.
static void** ImportSlot(const char* name)
{
    unsigned char* base = (unsigned char*)0x00400000;
    unsigned char* nt = base + *(unsigned*)(base + 0x3c);
    unsigned* desc = (unsigned*)(base + *(unsigned*)(nt + 0x80));
    for (; desc[3]; desc += 5)
    {
        if (_stricmp((const char*)base + desc[3], "MSVCR71.dll") != 0)
            continue;
        unsigned* names = (unsigned*)(base + (desc[0] ? desc[0] : desc[4]));
        for (unsigned k = 0; names[k]; k++)
            if (!(names[k] & 0x80000000) && strcmp((const char*)base + names[k] + 2, name) == 0)
                return (void**)(base + desc[4]) + k;
    }
    return 0;
}

// Points the import slot of `name` at `hook`; the CRT function goes to `*real`.
static int HookImport(const char* name, void* hook, void** real)
{
    void** slot = ImportSlot(name);
    if (!slot)
        return 0;
    *real = *slot;
    WriteCode((unsigned char*)slot, &hook, 4);
    return 1;
}

static void StartAllocLog(FILE* log)
{
    FILE* on = fopen("RBRRecompAllocLog.on", "rb");
    if (!on)
        return;
    fclose(on);
    if (memcmp((void*)0x0070bedb, "\xe8", 1) != 0 || *(unsigned*)0x0070bedc != 0x0070c3b4 - kNewMallocRet)
    {
        fprintf(log, "alloc log: operator new differs, not hooking\n");
        return;
    }
    s_allocs = (AllocRecord*)VirtualAlloc(0, kAllocMax * sizeof(AllocRecord), 0x3000 /* MEM_COMMIT | MEM_RESERVE */, 4);
    if (!s_allocs)
        return;
    int n = HookImport("malloc", (void*)LogMalloc, (void**)&s_malloc) + HookImport("calloc", (void*)LogCalloc, (void**)&s_calloc)
        + HookImport("realloc", (void*)LogRealloc, (void**)&s_realloc)
        + HookImport("_strdup", (void*)LogStrdup, (void**)&s_strdup)
        + HookImport("??3@YAXPAX@Z", (void*)LogDelete, (void**)&s_delete)
        + HookImport("??_V@YAXPAX@Z", (void*)LogDeleteArray, (void**)&s_deleteArray)
        + HookImport("free", (void*)LogFree, (void**)&s_free);
    unsigned long id;
    CreateThread(0, 0, WriteAllocs, 0, 0, &id);
    fprintf(log, "alloc log: %d of 7 imports hooked, writing RBRRecompAllocs.bin\n", n);
}

// ---- snapshots ----
// RBRRecompSnapshot.txt lists step counters. Right before the StepForward call that starts
// at a listed step S and the call after it (in playback after the control emit; the load
// settles step 40 at a time), every committed readable region of the process goes to
// RBRRecompSnap-<step>.bin, for harness/replay.py, with the registers at the call (eax ecx edx
// ebx esp ebp esi edi, esp pointing at the substeps argument), fxsave (x87 control word,
// MXCSR) and the stack from esp - kSnapBelow to esp + kSnapAbove exactly as the step sees it.
// A listed step is dumped again right after its call (RBRRecompSnap-<step>-after.bin, no
// stack copy).
// The step must see the same stack garbage as without snapshots (it copies uninitialised
// locals into the car), so SnapStepForward stands in for StepForward at the same esp, and the
// stack below it is put back after each dump; the hook's code is the same either way.
// File: region bytes, then the trailer: 'RBRS', 1, step, substeps, thread id, teb, regs[8], stack
// address, stack bytes, region count, fxsave[512], the stack bytes, region count x
// { base, size, file offset, protect, type, allocation base }; then the trailer's offset
// and 'RBRS' as the last 8 bytes.
extern "C" __declspec(dllimport) unsigned long __stdcall SetFilePointer(void* h, long lo, long* hi, unsigned long how);
static const unsigned kSnapBelow = 0x3ff00, kSnapAbove = 0x100;
static const unsigned kSnapStackBytes = kSnapBelow + kSnapAbove, kSnapMaxRegions = 16384;
static unsigned s_snapSteps[64], s_snapCount, s_snapNext;
static unsigned s_snapRegs[8], s_snapStackLo, s_snapSubsteps, s_snapStep, s_snapListed;
static unsigned s_snapVersion = 1;  // 2: a load-time dump (step = entry address, substeps = its ordinal)
static __declspec(align(16)) unsigned char s_snapFx[512];
struct SnapRegion
{
    unsigned base, size, offset, protect, type, allocBase;
};
// allocated only when snapshots are on: static buffers this size would shift the game's heap
static unsigned char* s_snapStack;  // kSnapStackBytes: the step's stack copy
static unsigned char* s_snapKeep;  // kSnapBelow: the stack below the step while a dump runs
static SnapRegion* s_snapRegions;
static void* s_snapFile;
static unsigned s_snapOffset, s_snapN, s_snapSkipped, s_snapFailed, s_snapMs;
static char s_snapName[64];

// 0: no snapshot; 1: the call after a listed step; 2: a listed step.
static int SnapWanted(unsigned step)
{
    int next = s_snapNext;
    s_snapNext = 0;
    for (unsigned i = 0; i < s_snapCount; i++)
        if (step == s_snapSteps[i])
            s_snapNext = 1;
    return s_snapNext ? 2 : next;
}

static int SnapWrite(void* h, const void* p, unsigned n)
{
    unsigned long done;
    return WriteFile(h, p, n, &done, 0) && done == n;
}

// Writes every committed readable region to h; returns the bytes written.
static unsigned SnapDump(void* h, unsigned& n, unsigned& skipped, unsigned& failed)
{
    unsigned offset = 0;
    n = skipped = failed = 0;
    MemInfo mi;
    for (unsigned at = 0x10000; at < 0x7fff0000 && VirtualQuery((void*)at, &mi, sizeof(mi)); at = mi.base + mi.size)
    {
        if (mi.state != 0x1000 /* MEM_COMMIT */)
            continue;
        // readable, not guard / no-access, and not write-combined or uncached (device memory)
        if (!(mi.protect & 0xee) || (mi.protect & 0x701))
        {
            skipped++;
            continue;
        }
        for (unsigned p = mi.base; p < mi.base + mi.size && n < kSnapMaxRegions;)
        {
            unsigned chunk = mi.base + mi.size - p < 0x100000 ? mi.base + mi.size - p : 0x100000;
            if (!SnapWrite(h, (void*)p, chunk))
            {
                SetFilePointer(h, (long)offset, 0, 0 /* FILE_BEGIN */);
                failed++;
                p += chunk;
                continue;
            }
            SnapRegion* r = n ? &s_snapRegions[n - 1] : 0;
            if (r && r->base >= mi.base && r->base + r->size == p && r->offset + r->size == offset)  // same region
                r->size += chunk;
            else
            {
                r = &s_snapRegions[n++];
                r->base = p, r->size = chunk, r->offset = offset, r->protect = mi.protect, r->type = mi.type;
                r->allocBase = mi.allocBase;
            }
            offset += chunk;
            p += chunk;
        }
    }
    return offset;
}

// Creates the file and writes the regions; the trailer follows in SnapClose.
static int SnapCreate(const char* name)
{
    char path[320];
    wsprintfA(s_snapName, "%s", name);
    s_snapFile = CreateFileA(GameFile(path, s_snapName), 0x40000000 /* GENERIC_WRITE */, 0, 0, 2 /* CREATE_ALWAYS */, 0x80, 0);
    if (s_snapFile == (void*)-1)
    {
        char line[128];
        int len = wsprintfA(line, "snapshot: %s NOT written (no file)\n", s_snapName);
        AppendLog(line, len);
        return 0;
    }
    unsigned start = GetTickCount();
    s_snapOffset = SnapDump(s_snapFile, s_snapN, s_snapSkipped, s_snapFailed);
    s_snapMs = GetTickCount() - start;
    return 1;
}

static int SnapOpen(const char* format, unsigned step)
{
    char name[64];
    wsprintfA(name, format, step);
    return SnapCreate(name);
}

// `stack`: whether the call's stack copy goes in.
static void SnapClose(int stack)
{
    unsigned teb, head[17] = { 0x53524252 /* 'RBRS' */, s_snapVersion, s_snapStep, s_snapSubsteps, GetCurrentThreadId() };
    __asm
    {
        mov eax, fs:[0x18]
        mov teb, eax
    }
    head[5] = teb;
    memcpy(head + 6, s_snapRegs, sizeof(s_snapRegs));
    head[14] = s_snapStackLo;
    head[15] = stack ? kSnapStackBytes : 0;
    head[16] = s_snapN;
    unsigned tail[2] = { s_snapOffset, 0x53524252 };
    void* h = s_snapFile;
    int ok = SnapWrite(h, head, sizeof(head)) && SnapWrite(h, s_snapFx, sizeof(s_snapFx))
        && (!stack || SnapWrite(h, s_snapStack, kSnapStackBytes))
        && SnapWrite(h, s_snapRegions, s_snapN * sizeof(SnapRegion)) && SnapWrite(h, tail, sizeof(tail));
    CloseHandle(h);
    if (s_allocs)
        AllocLog(9, s_snapSubsteps, s_snapVersion, s_snapStep, 0);
    char line[192];
    int len = wsprintfA(line, "snapshot: %s %s, %u regions, %u MB, %u skipped, %u chunks unreadable, %u ms\n", s_snapName,
        ok ? "written" : "WRITE FAILED", s_snapN, s_snapOffset >> 20, s_snapSkipped, s_snapFailed, s_snapMs);
    AppendLog(line, len);
}

// Before the step: nonzero when this call is dumped (regions written, trailer pending).
static int SnapBegin(void* self, unsigned substeps)
{
    if (!s_play)
        return 0;
    s_snapVersion = 1;  // a load-time dump before may have left 2
    s_snapStep = StepCounter(self);
    int snap = SnapWanted(s_snapStep);
    s_snapSubsteps = substeps;
    s_snapListed = snap == 2;
    return snap && SnapOpen("RBRRecompSnap-%u.bin", s_snapStep);
}

// After the step: the trailer, and for a listed step the state right after it.
static void SnapEnd()
{
    SnapClose(1);
    if (s_snapListed && SnapOpen("RBRRecompSnap-%u-after.bin", s_snapStep))
        SnapClose(0);
}

static unsigned s_snapLo, s_snapRet, s_snapOn, s_snapSave[3];
static const unsigned s_stepForward = kStepForward;

// [s_snapLo, s_snapLo + kSnapBelow) <-> s_snapKeep; uses ecx esi edi only.
static __declspec(naked) void SnapKeep()
{
    __asm
    {
        mov esi, s_snapLo
        mov edi, s_snapKeep
        mov ecx, 0x3ff00 / 4  // kSnapBelow
        cld
        rep movsd
        ret
    }
}

static __declspec(naked) void SnapRestore()
{
    __asm
    {
        mov esi, s_snapKeep
        mov edi, s_snapLo
        mov ecx, 0x3ff00 / 4
        cld
        rep movsd
        ret
    }
}

// StepForward's stand-in (thiscall: ecx = self, [esp + 4] = substeps, callee pops 4). The step
// is called from here at the esp it would have had (entry esp E: the return address is taken
// off and put back afterwards), and [E - kSnapBelow, E) is restored after SnapBegin and SnapEnd.
static __declspec(naked) void SnapStepForward()
{
    __asm
    {
        pop s_snapRet
        mov s_snapRegs[0], eax
        mov s_snapRegs[4], ecx
        mov s_snapRegs[8], edx
        mov s_snapRegs[12], ebx
        mov s_snapRegs[16], esp
        mov s_snapRegs[20], ebp
        mov s_snapRegs[24], esi
        mov s_snapRegs[28], edi
        fxsave s_snapFx
        lea eax, [esp - 4]
        lea edx, [esp - 4 - 0x3ff00]  // E - kSnapBelow
        mov s_snapLo, edx
    probe:  // commit the stack down to s_snapLo page by page (guard page) before copying it
        sub eax, 0x1000
        test dword ptr [eax], eax
        cmp eax, edx
        ja probe
        test dword ptr [edx], eax
        call SnapKeep
        push dword ptr [esp]
        push s_snapRegs[4]
        call SnapBegin
        add esp, 8
        mov s_snapOn, eax
        call SnapRestore
        fxrstor s_snapFx
        cmp s_snapOn, 0
        je step
        lea esi, [esp - 0x3ff00]  // esp - kSnapBelow
        mov s_snapStackLo, esi
        mov edi, s_snapStack
        mov ecx, (0x3ff00 + 0x100) / 4  // kSnapStackBytes
        cld
        rep movsd
    step:
        mov eax, s_snapRegs[0]
        mov ecx, s_snapRegs[4]
        mov edx, s_snapRegs[8]
        mov esi, s_snapRegs[24]
        mov edi, s_snapRegs[28]
        call s_stepForward
        cmp s_snapOn, 0
        je done
        mov s_snapSave[0], eax
        mov s_snapSave[4], esi
        mov s_snapSave[8], edi
        call SnapKeep
        call SnapEnd
        call SnapRestore
        mov eax, s_snapSave[0]
        mov esi, s_snapSave[4]
        mov edi, s_snapSave[8]
    done:
        mov edx, s_snapRet
        mov [esp - 8], edx  // E: the return address, as after a direct call
        jmp edx
    }
}

static void EnsureSnapBuffers()
{
    if (s_snapStack)
        return;
    s_snapStack = (unsigned char*)VirtualAlloc(0, kSnapStackBytes + kSnapBelow + kSnapMaxRegions * sizeof(SnapRegion),
        0x3000 /* MEM_COMMIT | MEM_RESERVE */, 0x04 /* PAGE_READWRITE */);
    if (!s_snapStack)
        return;
    s_snapKeep = s_snapStack + kSnapStackBytes;
    s_snapRegions = (SnapRegion*)(s_snapKeep + kSnapBelow);
}

// ---- load-time dumps: the entry and return stubs (see RBRRecompSnapAt.txt above) ----
// The stubs save the registers and the flags in globals before anything else, then copy the stack
// (from the call's esp - kSnapBelow to + kSnapAbove) before they push, so the stale bytes below the
// call stay as the original leaves them: the stack is put back from the copy. The only write to the
// stack is the return slot (E, the call's esp): the entry stub writes the return stub's address
// there, and the return stub puts the original return address back. The x87/SSE state is saved in
// s_snapAtFx. SnapAtDump writes the entry dump and asks the asm for the return hook
// (s_snapAtPatchSlot); SnapAtAfter writes the after dump and gives the asm the address to return to.
static void SnapAtDump()
{
    const unsigned* regs = s_snapAtRegs;
    unsigned k = s_snapAtIdx, entry = regs[3];  // the call's esp: its return address is at [entry]
    s_snapAtPatchSlot = 0;
    if (s_snapAtSeen[k] >= s_snapAtLimit[k])
        return;
    if (s_snapAtMaxValue[k] != 0)
    {
        unsigned arg = *(unsigned*)(entry + 4);
        if (!arg)
            return;
        unsigned value = *(unsigned*)(arg + s_snapAtArgOffset[k]);
        if (value < s_snapAtMin[k] || value > s_snapAtMaxValue[k])
            return;
    }
    unsigned ordinal = ++s_snapAtSeen[k];
    { char dbg[96]; int n = wsprintfA(dbg, "snapat: entry %08x ordinal %u esp %08x\n", s_snapAtAddr[k], ordinal, entry); AppendLog(dbg, n); }
    char name[64];
    wsprintfA(name, "RBRRecompSnapAt-%08x-%u.bin", s_snapAtAddr[k], ordinal);
    if (!SnapCreate(name))
        return;
    s_snapRegs[0] = regs[7], s_snapRegs[1] = regs[6], s_snapRegs[2] = regs[5], s_snapRegs[3] = regs[4];
    s_snapRegs[4] = entry, s_snapRegs[5] = regs[2], s_snapRegs[6] = regs[1], s_snapRegs[7] = regs[0];
    s_snapStackLo = entry - kSnapBelow;  // the copy starts there (SnapAtEntry)
    memcpy(s_snapFx, s_snapAtFx, sizeof(s_snapFx));
    s_snapVersion = 2, s_snapStep = s_snapAtAddr[k], s_snapSubsteps = ordinal;
    SnapClose(1);
    if (s_snapAtDepth < kSnapAtDepth)
    {
        unsigned d = s_snapAtDepth++;
        s_snapAtRetAddr[d] = *(unsigned*)entry;  // the return address the call will return to
        s_snapAtRetOrd[d] = ordinal;
        s_snapAtRetIdx[d] = k;
        s_snapAtPatchSlot = entry;
    }
}

// On return: the after dump (the registers as the function returns them, no stack). Returns the
// address the function returns to.
static unsigned SnapAtAfter()
{
    const unsigned* regs = s_snapAtRegs;  // esp is the caller's: the return address is popped
    unsigned d = --s_snapAtDepth, k = s_snapAtRetIdx[d];
    { char dbg[96]; int n = wsprintfA(dbg, "snapat: return %08x esp %08x\n", s_snapAtRetAddr[d], regs[3]); AppendLog(dbg, n); }
    char name[80];
    wsprintfA(name, "RBRRecompSnapAt-%08x-%u-after.bin", s_snapAtAddr[k], s_snapAtRetOrd[d]);
    s_snapRegs[0] = regs[7], s_snapRegs[1] = regs[6], s_snapRegs[2] = regs[5], s_snapRegs[3] = regs[4];
    s_snapRegs[4] = regs[3], s_snapRegs[5] = regs[2], s_snapRegs[6] = regs[1], s_snapRegs[7] = regs[0];
    s_snapStackLo = regs[3] - kSnapBelow;
    memcpy(s_snapFx, s_snapAtFx, sizeof(s_snapFx));
    s_snapVersion = 2, s_snapStep = s_snapAtAddr[k], s_snapSubsteps = s_snapAtRetOrd[d];
    if (SnapCreate(name))
        SnapClose(0);
    return s_snapAtRetAddr[d];
}

// The entry stub: the trampoline jumps here with esp = E (the call's esp, [E] the return address).
// When the dumps for this function are done it goes straight to the function's body.
static __declspec(naked) void SnapAtEntry()
{
    __asm
    {
        mov s_snapAtRegs[0], edi
        mov s_snapAtRegs[4], esi
        mov s_snapAtRegs[8], ebp
        mov s_snapAtRegs[12], esp
        mov s_snapAtRegs[16], ebx
        mov s_snapAtRegs[20], edx
        mov s_snapAtRegs[24], ecx
        mov s_snapAtRegs[28], eax
        lahf
        mov byte ptr [s_snapAtFlags], ah
        seto al
        mov byte ptr [s_snapAtFlags + 1], al
        mov eax, [s_snapAtIdx]
        mov edx, [s_snapAtSeen + eax * 4]
        cmp edx, [s_snapAtLimit + eax * 4]
        jae snapat_out
        lea edx, [esp - 0x3ff00]  // E - kSnapBelow
        mov s_snapLo, edx
        mov eax, esp
    snapat_probe:  // commit the stack down to s_snapLo page by page before copying it
        sub eax, 0x1000
        test dword ptr [eax], eax
        cmp eax, edx
        ja snapat_probe
        test dword ptr [edx], eax
        mov esi, edx
        mov edi, s_snapStack
        mov ecx, (0x3ff00 + 0x100) / 4  // kSnapStackBytes
        rep movsd
        fxsave s_snapAtFx
        call SnapAtDump
        mov esi, s_snapStack
        mov edi, s_snapLo
        mov ecx, 0x3ff00 / 4
        rep movsd
        mov edx, s_snapAtPatchSlot
        test edx, edx
        jz snapat_nohook
        mov ecx, s_snapAtHook
        mov [edx], ecx  // the return address: the return stub
    snapat_nohook:
        fxrstor s_snapAtFx
    snapat_out:
        mov al, byte ptr [s_snapAtFlags + 1]
        add al, 0x7f  // sets OF when al is 1
        mov ah, byte ptr [s_snapAtFlags]
        sahf
        mov edi, s_snapAtRegs[0]
        mov esi, s_snapAtRegs[4]
        mov ebp, s_snapAtRegs[8]
        mov ebx, s_snapAtRegs[16]
        mov edx, s_snapAtRegs[20]
        mov ecx, s_snapAtRegs[24]
        mov eax, s_snapAtRegs[28]
        jmp dword ptr [s_snapAtTarget]
    }
}

// The return stub: the function returns to it with esp = E + 4. It saves the registers and the flags,
// copies the stack, dumps the state after the call, then puts the stack back from the copy and the
// original return address into its slot (E), so the ret below returns to the caller as the original does.
static __declspec(naked) void SnapAtRet()
{
    __asm
    {
        mov s_snapAtRegs[0], edi
        mov s_snapAtRegs[4], esi
        mov s_snapAtRegs[8], ebp
        mov s_snapAtRegs[12], esp
        mov s_snapAtRegs[16], ebx
        mov s_snapAtRegs[20], edx
        mov s_snapAtRegs[24], ecx
        mov s_snapAtRegs[28], eax
        lahf
        mov byte ptr [s_snapAtFlags], ah
        seto al
        mov byte ptr [s_snapAtFlags + 1], al
        lea edx, [esp - 0x3ff00]  // E + 4 - kSnapBelow
        mov s_snapLo, edx
        mov eax, esp
    snapatret_probe:
        sub eax, 0x1000
        test dword ptr [eax], eax
        cmp eax, edx
        ja snapatret_probe
        test dword ptr [edx], eax
        mov esi, edx
        mov edi, s_snapStack
        mov ecx, (0x3ff00 + 0x100) / 4
        rep movsd
        fxsave s_snapAtFx
        call SnapAtAfter
        mov s_snapAtJump, eax
        mov esi, s_snapStack
        mov edi, s_snapLo
        mov ecx, 0x3ff00 / 4
        rep movsd
        fxrstor s_snapAtFx
        mov al, byte ptr [s_snapAtFlags + 1]
        add al, 0x7f
        mov ah, byte ptr [s_snapAtFlags]
        sahf
        push dword ptr [s_snapAtJump]  // into the return slot (E): the ret pops it
        mov edi, s_snapAtRegs[0]
        mov esi, s_snapAtRegs[4]
        mov ebp, s_snapAtRegs[8]
        mov ebx, s_snapAtRegs[16]
        mov edx, s_snapAtRegs[20]
        mov ecx, s_snapAtRegs[24]
        mov eax, s_snapAtRegs[28]
        ret
    }
}

static unsigned SnapAtEntryAddress()
{
    return (unsigned)&SnapAtEntry;
}

static unsigned SnapAtRetAddress()
{
    return (unsigned)&SnapAtRet;
}

static void LoadSnapshots(FILE* log)
{
    FILE* f = fopen("RBRRecompSnapshot.txt", "r");
    if (!f)
        return;
    while (s_snapCount < 64 && fscanf(f, "%u", &s_snapSteps[s_snapCount]) == 1)
        s_snapCount++;
    fclose(f);
    EnsureSnapBuffers();
    if (!s_snapStack)
        s_snapCount = 0;
    if (s_snapCount)
        s_snapTarget = (unsigned)SnapStepForward;
    fprintf(log, "snapshot: %u steps listed (and each one's next step)\n", s_snapCount);
}

static const Control* PlaybackControl(unsigned step)
{
    while (s_playPos < s_playCount && s_play[s_playPos].step < step)
        s_playPos++;
    if (s_playPos < s_playCount && s_play[s_playPos].step == step)
        return &s_play[s_playPos];
    // the counter restarted (new stage): search from the beginning
    for (s_playPos = 0; s_playPos < s_playCount && s_play[s_playPos].step != step; s_playPos++)
        ;
    return s_playPos < s_playCount ? &s_play[s_playPos] : 0;
}

// Replaces the vtable slot: thiscall(this, substeps) callee-pops 4, the same as
// this __fastcall shape with an unused edx.
static void __fastcall StepForwardHook(void* self, int, unsigned substeps)
{
    if (!s_play || substeps > 8)  // record, or a load-time settle (no input then)
    {
        unsigned pre = StepCounter(self);
        unsigned char control[kControlBytes];
        memcpy(control, s_lastControl, kControlBytes);
        s_stepSelf = self;
        s_stepDepth++;
        CallStepForward(self, substeps);
        s_stepDepth--;
        if (s_rec)
            Record(pre, StepCounter(self), substeps, control);
        if (s_peek)
            PeekRecord(StepCounter(self));
        return;
    }
    // Fast forward: inputs are keyed by step, not time, so run s_fastForward steps for
    // every step the frame asked for; only the picture skips ahead.
    for (unsigned i = 0; i < substeps * s_fastForward; i++)
    {
        unsigned pre = StepCounter(self);
        s_lastStepTick = GetTickCount();
        if (s_playCount && !s_writeWatch && pre > s_play[s_playCount - 1].step + 200)  // 2 s past the last control
            Quit();
        const Control* c = PlaybackControl(pre);
        if (c && s_emitThis)
            CallEmit(s_emitThis, c->data);
        s_stepSelf = self;
        s_stepDepth++;
        CallStepForward(self, 1);
        s_stepDepth--;
        if (s_rec)
            Record(pre, StepCounter(self), 1, c ? c->data : s_lastControl);
        if (s_peek)
            PeekRecord(StepCounter(self));
    }
}

// Returns nonzero to let a player emit through.
static int __stdcall OnPlayerControl(void* self, const void* sig)
{
    s_emitThis = self;
    memcpy(s_lastControl, sig, kControlBytes);
    return !s_play;
}

// Entry detour of Emit. Player emits (returning to 0x0048d253) go to OnPlayerControl;
// everything continues with the displaced prologue unless it says drop.
static __declspec(naked) void EmitHook()
{
    __asm
    {
        cmp dword ptr [esp], 0x0048d253
        jne resume
        push ecx
        push dword ptr [esp + 8]
        push ecx
        call OnPlayerControl
        pop ecx
        test eax, eax
        jnz resume
        ret 4
    resume:
        push edi
        mov edi, ecx
        mov eax, [edi + 4]
        push 0x0048cd96
        ret
    }
}

// NGP (Plugins/NGP.dll, notes/ngp.md) delivers the player's controls itself: from
// CSigCarControlFiltered it builds the payload, hands it straight to the car's control input
// (the function the car's CSigCarControl connection calls without NGP) and runs the other
// listeners through its own walk, so Emit never sees them. With NGP loaded that input is hooked
// instead, for the player's car and calls from NGP.dll: what it gets is what the physics gets, so
// it is what the recorder stores and what playback feeds back (past the hook). The other
// listeners (sound, driver animation, HedgeHog3D, rally school) keep getting NGP's live controls
// in playback.
static __declspec(naked) void CarControlHook()
{
    __asm
    {
        mov eax, dword ptr [esp]
        cmp eax, s_ngpLo
        jb resume
        cmp eax, s_ngpHi
        jae resume
        mov eax, dword ptr ds:[0x008ef660]  // kCarPtr
        cmp ecx, eax
        jne resume
        push ecx
        push dword ptr [esp + 8]
        push ecx
        call OnPlayerControl
        pop ecx
        test eax, eax
        jnz resume
        ret 4
    resume:
        jmp CarControlResume
    }
}

static void HookCarControl(FILE* log)
{
    const char* ngp = (const char*)GetModuleHandleA("NGP.dll");
    if (s_carControlHooked || !ngp)
        return;
    // SizeOfImage: the PE header (e_lfanew at 0x3c), then 0x50 into it
    s_ngpLo = (unsigned)ngp;
    s_ngpHi = s_ngpLo + *(const unsigned*)(ngp + *(const unsigned*)(ngp + 0x3c) + 0x50);
    if (memcmp((void*)kCarControl, kCarControlPrologue, sizeof(kCarControlPrologue)) != 0)
    {
        fprintf(log, "recorder: NGP.dll loaded but the car's control input differs; player controls not hooked\n");
        return;
    }
    unsigned char jmp[6] = { 0xE9, 0, 0, 0, 0, 0x90 };
    *(unsigned*)(jmp + 1) = (unsigned)&CarControlHook - (kCarControl + 5);
    WriteCode((unsigned char*)kCarControl, jmp, 6);
    s_carControlHooked = 1;
    fprintf(log, "recorder: NGP.dll delivers the player's controls; hooked the car's control input at %08x\n", kCarControl);
}

static void LoadPlayback(FILE* log)
{
    FILE* f = fopen("RBRRecompPlayback.bin", "rb");
    if (!f)
        return;
    unsigned cap = 1 << 16, head[5];
    s_play = new Control[cap];
    while (fread(head, 4, 5, f) == 5 && head[0] == 0x314d5246)
    {
        if (s_playCount == cap)
        {
            Control* more = new Control[cap * 2];
            memcpy(more, s_play, cap * sizeof(Control));
            delete[] s_play;
            s_play = more;
            cap *= 2;
        }
        Control& c = s_play[s_playCount];
        if (fread(c.data, 1, kControlBytes, f) != kControlBytes)
            break;
        c.step = head[1];
        if (head[3] == 1)  // only single steps carry a control that fed exactly one step
            s_playCount++;
        fseek(f, head[4] + 4 * kWheelBytes, SEEK_CUR);
    }
    fclose(f);
    fprintf(log, "playback: %u controls from RBRRecompPlayback.bin\n", s_playCount);
}

// ---- failure handling for unattended runs ----
// A pure virtual call (msvcr71 _purecall, R6025) or any other modal error box would
// hang an unattended run. The exe's IAT slot for _purecall (0x00729534) is pointed at
// OnPureCall, which logs the caller and quits; a watchdog thread quits a playback
// whose physics stops advancing for 20 s (or never starts within 80 s).
static const unsigned kPurecallSlot = 0x00729534;

// Logs and kills the game. Win32 only and no ExitProcess (DLL detach can block under
// Wine): the stuck thread may hold any CRT lock. A failed run's recording may lose its tail.
static void Fail(const char* what, unsigned addr)
{
    char buf[512], *p = buf;
    p += wsprintfA(p, "%s at %08x", what, addr);
    p = BlobAddress(p, addr);
    if (s_shadowIndex >= 0)
        p += wsprintfA(p, "; last shadowed call %08x, phase %d", s_funcs[s_shadowIndex].addr, s_shadowPhase);
    p += wsprintfA(p, "\n");
    AppendLog(buf, p - buf);
    TerminateProcess(GetCurrentProcess(), 3);
}

static void __cdecl OnPureCall()
{
    unsigned caller = 0;
    __asm
    {
        mov eax, [ebp + 4]
        mov caller, eax
    }
    Fail("pure virtual call from", caller);
}

// " [offset]address (our copy ...)" for each code address (exe .text or our copies) in words.
static char* CodeWords(char* p, const unsigned* words, unsigned n, unsigned max)
{
    for (unsigned k = 0, found = 0; k < n && found < max; k++)
    {
        unsigned v = words[k];
        if ((v >= 0x00401000 && v < 0x00729000) || v - (unsigned)s_blob < s_blobSize)
        {
            p += wsprintfA(p, " [%x]%08x", k * 4, v);
            p = BlobAddress(p, v);
            found++;
        }
    }
    return p;
}

// Where a stalled main thread is: eip samples and the code addresses on its stack, or that it
// is gone (a stack overflow under Wine), with the overflow's stack when OnException saw it.
static void LogStall()
{
    static char buf[16384];
    char* p = buf;
    unsigned long code = 0;
    if (!s_mainThread)
        p += wsprintfA(p, "  main thread handle unavailable (tid %08x)\n", s_mainThreadId);
    else if (GetExitCodeThread(s_mainThread, &code) && code != 259 /* STILL_ACTIVE */)
        p += wsprintfA(p, "  main thread is gone (exit code %08x)\n", code);
    else if (s_mainThread)
    {
        unsigned ctx[0x2cc / 4];
        int ok = 0;
        p += wsprintfA(p, "  main thread eip:");
        for (int k = 0; k < 8; k++)
        {
            if (SuspendThread(s_mainThread) == (unsigned long)-1)
            {
                p += wsprintfA(p, " suspend failed (%08x)", GetLastError());
                break;
            }
            ctx[0] = 0x10007;  // CONTEXT_FULL
            ok = GetThreadContext(s_mainThread, ctx);
            unsigned long contextError = ok ? 0 : GetLastError();
            ResumeThread(s_mainThread);
            if (!ok)
            {
                p += wsprintfA(p, " context failed (%08x)", contextError);
                break;
            }
            p += wsprintfA(p, " %08x", ctx[0xb8 / 4]);
            p = BlobAddress(p, ctx[0xb8 / 4]);
            Sleep(150);
        }
        if (ok)
        {
            unsigned* esp = (unsigned*)ctx[0xc4 / 4];
            unsigned n = 0;
            while (n < 4096 && !IsBadReadPtr(esp + n, 4))
                n++;
            p += wsprintfA(p, "\n  main thread stack:");
            p = CodeWords(p, esp, n, 48);
        }
        p += wsprintfA(p, "\n");
    }
    if (s_overflowEsp)
    {
        p += wsprintfA(p, "  stack overflow at %08x", s_overflowEip);
        p = BlobAddress(p, s_overflowEip);
        p += wsprintfA(p, ", stack:");
        p = CodeWords(p, (const unsigned*)s_overflowStack, 2048, 40);
        p += wsprintfA(p, "\n");
    }
    AppendLog(buf, p - buf);
}

static unsigned long __stdcall Watchdog(void*)
{
    for (;;)
    {
        Sleep(1000);
        if ((int)(GetTickCount() - s_lastStepTick) > 20000)
        {
            char buf[128];
            AppendLog(buf, wsprintfA(buf, "physics stalled for 20 s (hung, not crashed); last step tick %08x\n",
                s_lastStepTick));
            LogStall();
            TerminateProcess(GetCurrentProcess(), 3);
        }
    }
}

static void StartFailureHandling(FILE* log)
{
    unsigned hook = (unsigned)&OnPureCall;
    WriteCode((unsigned char*)kPurecallSlot, &hook, 4);
    s_lastStepTick = GetTickCount() + 60000;  // the stage load gets 80 s in all
    unsigned long id;
    CreateThread(0, 0, Watchdog, 0, 0, &id);
    fprintf(log, "unattended: _purecall hooked, watchdog running\n");
}

// E03WndProc (0x00447ab0) sends WM_ACTIVATE/WA_INACTIVE to CPC_Application::WndProc,
// which clears 0x007c0bbc. E04's main loop runs render and physics only while that
// flag is nonzero. Subclass its window procedure and eat focus-loss messages before
// they reach the game's pause gate; playback still supplies controls while unfocused.
static const unsigned kGameWndProc = 0x00447ab0;
static long s_gameWndProc;
static volatile int s_focusAttached;
static unsigned s_focusMessageLogged[3];

static long __stdcall KeepRunningWndProc(void* hwnd, unsigned msg, unsigned w, long l)
{
    unsigned kind = 3;
    const char* name = 0;
    if (msg == 0x001c && w == 0)
    {
        kind = 0;
        name = "WM_ACTIVATEAPP(FALSE)";
    }
    else if (msg == 0x0006 && (w & 0xffff) == 0)
    {
        kind = 1;
        name = "WM_ACTIVATE(WA_INACTIVE)";
    }
    else if (msg == 0x0008)
    {
        kind = 2;
        name = "WM_KILLFOCUS";
    }
    if (kind < 3)
    {
        if (s_focusMessageLogged[kind]++ == 0)
        {
            char line[128];
            wsprintfA(line, "unfocused: swallowed %s\n", name);
            AppendLog(line, (unsigned)strlen(line));
        }
        return 0;
    }
    return CallWindowProcA((void*)s_gameWndProc, hwnd, msg, w, l);
}

static int __stdcall AttachGameWindow(void* hwnd, long)
{
    unsigned long pid = 0;
    unsigned long tid = GetWindowThreadProcessId(hwnd, &pid);
    if (pid != GetCurrentProcessId())
        return 1;
    long proc = GetWindowLongA(hwnd, -4 /* GWL_WNDPROC */);
    if (proc == (long)&KeepRunningWndProc)
    {
        s_focusAttached = 1;
        return 0;
    }
    if ((unsigned)proc != kGameWndProc)
        return 1;

    s_gameWndProc = proc;
    long old = SetWindowLongA(hwnd, -4 /* GWL_WNDPROC */, (long)&KeepRunningWndProc);
    if ((unsigned)old == kGameWndProc)
    {
        s_mainThreadId = tid;
        // GetExitCodeThread, Get/SetThreadContext, and SuspendThread require these
        // four rights. THREAD_ALL_ACCESS can fail on Wine where the smaller
        // query/context/suspend mask succeeds.
        s_mainThread = OpenThread(0x005a /* QUERY_INFORMATION | GET/SET_CONTEXT | SUSPEND_RESUME */, 0, tid);
        if (s_writeWatch && s_mainThread)
        {
            s_watchArmed = ArmWriteWatchThread(s_mainThread);
            s_watchThreadId = tid;
        }
        char line[128];
        if (s_mainThread)
            wsprintfA(line, "unfocused: subclass installed on window %08x, main thread %08x\n", (unsigned)hwnd, tid);
        else
            wsprintfA(line, "unfocused: subclass installed on window %08x; OpenThread(%08x) failed (%08x)\n",
                (unsigned)hwnd, tid, GetLastError());
        AppendLog(line, (unsigned)strlen(line));
        if (s_writeWatch)
        {
            char watch[256];
            if (s_watchArmed && s_watchListed)
                wsprintfA(watch, "write-watch armed on main thread %08x: %u listed words\n", s_watchThreadId, s_watchCount);
            else if (s_watchArmed)
                wsprintfA(watch, "write-watch armed on main thread %08x: tracked %08x=%08x %08x=%08x, current %08x=%08x %08x=%08x, marker %08x\n",
                    s_watchThreadId, kWatchLookahead, s_watchWords[0], kWatchPaceSpeed, s_watchWords[1],
                    kWatchLookahead, *(volatile unsigned*)kWatchLookahead, kWatchPaceSpeed,
                    *(volatile unsigned*)kWatchPaceSpeed, kWatchMarkerTick);
            else
                wsprintfA(watch, "write-watch could not arm on main thread %08x (error %08x)\n", tid, GetLastError());
            AppendLog(watch, (unsigned)strlen(watch));
        }
        s_focusAttached = 1;
        return 0;
    }
    // If another component changed the proc between the read and write, restore it.
    if (old != 0)
        SetWindowLongA(hwnd, -4 /* GWL_WNDPROC */, old);
    return 1;
}

static unsigned long __stdcall AttachGameWindowThread(void*)
{
    for (unsigned i = 0; i < 600; i++)
    {
        s_focusAttached = 0;
        EnumWindows(AttachGameWindow, 0);
        if (s_focusAttached)
            return 0;
        Sleep(100);
    }
    {
        const char* line = "unfocused: game window proc 00447ab0 was not found\n";
        AppendLog(line, (unsigned)strlen(line));
    }
    return 0;
}

static void StartFocusHandling(FILE* log)
{
    unsigned long id = 0;
    void* thread = CreateThread(0, 0, AttachGameWindowThread, 0, 0, &id);
    if (!thread)
    {
        fprintf(log, "unfocused: could not start window-proc subclass thread\n");
        return;
    }
    CloseHandle(thread);
    fprintf(log, "unfocused: waiting to subclass game window proc\n");
}

static void StartRecorder(FILE* log)
{
    FILE* on = fopen("RBRRecompRecord.on", "rb");
    if (on)
        fclose(on);
    LoadPlayback(log);
    FILE* ff = fopen("RBRRecompFastForward.txt", "r");
    if (ff)
    {
        unsigned n = 0;
        if (fscanf(ff, "%u", &n) == 1 && n >= 1 && n <= 64)
            s_fastForward = n;
        fclose(ff);
        fprintf(log, "playback: %u steps per requested step\n", s_fastForward);
    }
    LoadSnapshots(log);
    LoadPeeks(log);
    if (!on && !s_play && !s_peekCount)
        return;
    if (*(unsigned*)kStepSlot != kStepForward || ((on || s_play) && memcmp((void*)kEmit, kEmitPrologue, 6) != 0))
    {
        fprintf(log, "recorder: StepForward slot or Emit prologue differ, not hooking\n");
        ClosePeek();
        return;
    }
    if (on || s_play)
    {
        char name[64];
        sprintf(name, "RBRRecompRecord-%lu.bin", GetTickCount());
        s_rec = fopen(name, "wb");
        LoadEarlyStop(log);
    }
    unsigned hook = (unsigned)&StepForwardHook;
    WriteCode((unsigned char*)kStepSlot, &hook, 4);
    if (on || s_play)
    {
        unsigned char jmp[6] = { 0xE9, 0, 0, 0, 0, 0x90 };
        *(unsigned*)(jmp + 1) = (unsigned)&EmitHook - (kEmit + 5);
        WriteCode((unsigned char*)kEmit, jmp, 6);
        HookCarControl(log);
        fprintf(log, "recorder: writing RBRRecompRecord-<tick>.bin%s\n", s_play ? " (playback)" : "");
    }
    if (s_play)
        StartFailureHandling(log);
}

static void StartWriteWatch(FILE* log)
{
    FILE* on = fopen("RBRRecompWriteWatch.on", "rb");
    if (!on)
        return;
    unsigned n = 0;
    char tok[64];
    while (fscanf(on, "%63s", tok) == 1)
    {
        unsigned v;
        if (!strcmp(tok, "steps"))
            fscanf(on, "%u", &s_watchSteps);
        else if (!strcmp(tok, "car"))
            s_watchCar = 1;  // the four wheels' +0xa3c words, followed from the car (RefreshWatchWheels)
        else if (n < 4 && sscanf(tok, "%x", &v) == 1)
            s_watchAddr[n++] = v;
    }
    if (s_watchCar)
        n = 4;
    if (n)
    {
        s_watchListed = 1;
        s_watchCount = n;
    }
    fclose(on);
    s_writeWatch = 1;
    EnsureExceptionHandler();
    unsigned long id = 0;
    void* thread = CreateThread(0, 0, ArmWriteWatchThreads, 0, 0, &id);
    if (thread)
        CloseHandle(thread);
    if (s_watchListed)
        fprintf(log, "write-watch: listed %u words, steps %u (0: frame cap)\n", s_watchCount, s_watchSteps);
    else
        fprintf(log, "write-watch: requested lookahead/speed writes and marker-tick reads\n");
}

// ---- auto start ----
// CApplication::Tic 0x004804f0 runs once per frame on the main thread. Its first 7
// bytes (push -1; push 0x712358) move to a trampoline; the detour calls AutoStart.
static const unsigned kTic = 0x004804f0;
static const unsigned char kTicPrologue[7] = { 0x6a, 0xff, 0x68, 0x58, 0x23, 0x71, 0x00 };
static IRBRGame* s_game;
static int s_auto[6];  // map car weather tyre [damage [transmission]]; -1: leave the game's setting
static const unsigned kDamage = 0x01660850;  // next-stage settings 0x01660800 +0x50: 0 off, 1 safe, 2 reduced, 3 realistic
static const unsigned kTransmission = 0x01660814;  // +0x14: 0 manual, 1 automatic (the options menu's 2-entry item)
static unsigned s_autoNext;
// Or a replay (RBRRecompReplay.txt: a file name in Replays\), started as the replay menu does (0x57fe30):
// CReplayManager::StreamFetch 0x4999b0 on the manager at 0x893634 (also NGPCarMenu's RBRAPI_Replay).
static char s_replay[260];
static unsigned s_replaySize;
static int s_replayFetched;
typedef int (__fastcall* StreamFetchFn)(void* self, void*, const char* name, int* p3, int* p4, unsigned size);

static void __stdcall AutoStart()
{
    if (!s_autoNext || GetTickCount() < s_autoNext)
        return;
    if (s_writeWatch && !s_watchArmed)
    {
        s_autoNext = GetTickCount() + 100;
        return;
    }
    s_autoNext = GetTickCount() + 250;  // the menus may not take it yet (StartGame returns false): retry
    if (s_replay[0])
    {
        // Loaded, the replay waits in app state 10 (camera spin) for the menu's "Replay": start it as that does,
        // CApplication::M0047ca40 (arms the start; the next CApplication::Tic enters state 8). The app states are
        // logged until state 8 (under NGP it goes to 9 at once, notes/ngp.md Replays).
        void* app = *(void**)0x007eac48;
        static int lastState = -1;
        int state = app ? *(int*)((char*)app + 0x728) : -1;
        if (state != lastState)
        {
            char text[64];
            AppendLog(text, wsprintfA(text, "replay: app state %d\n", state));
            lastState = state;
        }
        if (s_replayFetched == 1 && state == 10)
        {
            ((void (__fastcall*)(void*))0x0047ca40)(app);
            s_replayFetched = 2;
        }
        if (s_replayFetched)
        {
            if (state == 8)
                s_autoNext = 0;
            return;
        }
        int p3, p4;
        void* manager = *(void**)0x00893634;
        if (manager && ((StreamFetchFn)0x004999b0)(manager, 0, s_replay, &p3, &p4, s_replaySize))
            s_replayFetched = 1;
        return;
    }
    if (s_auto[4] >= 0)
        *(int*)kDamage = s_auto[4];
    if (s_auto[5] >= 0)
        *(int*)kTransmission = s_auto[5];
    if (s_game->StartGame(s_auto[0], s_auto[1], (IRBRGame::ERBRWeatherType)s_auto[2],
            (IRBRGame::ERBRTyreTypes)s_auto[3], 0))
        s_autoNext = 0;
}

// The car and the watched words at the first frame of the stage, to check the addresses
// against the snapshot (car at kCarPtr; the words are whatever the listed addresses hold).
static void LogWatchSample()
{
    static char buf[512];
    char* p = buf;
    p += wsprintfA(p, "write-watch sample frame %u step %u car %08x", s_watchFrame,
        s_stepSelf ? StepCounter(s_stepSelf) : 0, *(volatile unsigned*)kCarPtr);
    for (unsigned i = 0; i < s_watchCount; i++)
        p += wsprintfA(p, " word %08x=%08x", s_watchAddr[i], *(volatile unsigned*)s_watchAddr[i]);
    p += wsprintfA(p, "\n");
    AppendLog(buf, p - buf);
}

// Car mode: the car at kCarPtr holds its four wheel pointers at +0x470; the watched word is
// wheel+0xa3c. A new car (or a new wheel address) re-arms the main thread from a helper thread:
// SuspendThread cannot run on the thread that calls it, and the main thread runs the Tic.
static const unsigned kWheelTable = 0x470, kWheelWord = 0xa3c;

static unsigned long __stdcall ArmMainWatchThread(void*)
{
    ArmWriteWatchThread(s_mainThread);
    return 0;
}

static void RefreshWatchWheels()
{
    unsigned car = *(volatile unsigned*)kCarPtr;
    if (!car || IsBadReadPtr((void*)(car + kWheelTable), 16))
        return;
    unsigned addr[4];
    for (unsigned k = 0; k < 4; k++)
    {
        unsigned wheel = *(volatile unsigned*)(car + kWheelTable + 4 * k);
        if (!wheel || IsBadReadPtr((void*)(wheel + kWheelWord), 4))
            return;
        addr[k] = wheel + kWheelWord;
    }
    if (!memcmp(addr, s_watchAddr, sizeof(addr)))
        return;
    char line[256];
    char* p = line;
    p += wsprintfA(p, "write-watch wheels: frame %u step %u car %08x", s_watchFrame,
        s_stepSelf ? StepCounter(s_stepSelf) : 0, car);
    for (unsigned k = 0; k < 4; k++)
    {
        s_watchAddr[k] = addr[k];
        s_watchSeen[k] = 0;
        p += wsprintfA(p, " wheel%u+a3c %08x", k, addr[k]);
    }
    p += wsprintfA(p, "\n");
    AppendLog(line, p - line);
    unsigned long id = 0;
    void* thread = CreateThread(0, 0, ArmMainWatchThread, 0, 0, &id);
    if (thread)
        CloseHandle(thread);
}

static void WatchGameFrame()
{
    if (!s_writeWatch || !s_watchArmed)
        return;
    if (s_watchCar)
        RefreshWatchWheels();
    if (!s_watchStageStarted)
        return;
    s_lastStepTick = GetTickCount();  // the playback intentionally continues after its last input
    unsigned frame = ++s_watchFrame;
    if (frame == 1 && s_watchListed)
        LogWatchSample();
    if (frame > 5000 && !s_watchSteps)
        Quit();  // the marker tick for frame 5000 has completed
    if (s_watchSteps && s_stepSelf && StepCounter(s_stepSelf) >= s_watchSteps)
        Quit();  // checked per frame: the hook also runs during load, before its physics object is set up
}

// RBRRecompCamera.txt (a camera mode, e.g. 1 chase or 5 Cam_internal; notes/cameras.md 2a): in the race (app state
// 1) each frame whose mode differs steps the camera as the CH_CAM key does (0x487840 on CApplication::GetUnk70
// 0x47ca00), so a playback can be heard from that view (the key needs the focus, which an unattended run lacks).
static int s_cameraMode = -1;

static void LoadCamera(FILE* log)
{
    FILE* f = fopen("RBRRecompCamera.txt", "r");
    if (!f)
        return;
    if (fscanf(f, "%d", &s_cameraMode) != 1 || s_cameraMode < 1 || s_cameraMode > 5 || s_cameraMode == 2)
        s_cameraMode = -1;
    fclose(f);
    fprintf(log, "camera: mode %d in the race\n", s_cameraMode);
}

// RBRRecompVolumes.txt: the volume options at 0x01660e08 + 4 k (engine, co-driver, effects, music, tyres, replay
// music; notes/sound.md 2.2), a number or - (keep) each. The values the game has are logged at the first race frame;
// the listed ones are written every frame from then on, so the original can be heard one group at a time.
static float s_volumes[6];
static int s_volumeSet[6], s_volumesLogged;

static void LoadVolumes(FILE* log)
{
    FILE* f = fopen("RBRRecompVolumes.txt", "r");
    if (!f)
        return;
    for (int k = 0; k < 6; k++)
    {
        char word[32];
        if (fscanf(f, "%31s", word) != 1)
            break;
        s_volumeSet[k] = sscanf(word, "%f", &s_volumes[k]) == 1;
        if (s_volumeSet[k])
            fprintf(log, "volumes: option %d (0x%08x) set to %.3f\n", k, 0x01660e08 + 4 * k, s_volumes[k]);
    }
    fclose(f);
}

// Diagnostic (sound A/B): CCarSound::M6a0's call of Mf280 (0x4b0720), each EngineNoise emit, logs the emits with a
// non-zero misfire output.
static const unsigned kPopCall = 0x004b0720;
static unsigned s_popEmits;
static unsigned s_fpsLimit, s_ticFrames;
static int s_popsOn;

// And E30DXMixer::Vf4c's sounds made (0x4cf8cb) and refused at the channel limit (0x4cf878, the silent stand-in),
// with the live count and the limit every 1000 emits.
static unsigned s_soundsMade, s_soundsRefused;

// E30DXMixer::Vf48 by the caller of E32Factory::Make3 (its return address), when Make3 called it.
static unsigned s_callers[48][3];

static void __stdcall SoundBy(unsigned* frame, unsigned refused)
{
    unsigned caller = frame[6] == 0x004db58e ? frame[12] : frame[6];
    for (int k = 0; k < 48; k++)
    {
        if (s_callers[k][0] == caller || s_callers[k][0] == 0)
        {
            s_callers[k][0] = caller;
            s_callers[k][1 + refused]++;
            return;
        }
    }
}

static void LogCallers()
{
    char text[96];
    for (int k = 0; k < 48 && s_callers[k][0]; k++)
        AppendLog(text, wsprintfA(text, "sounds: caller %08x made %u refused %u\n", s_callers[k][0], s_callers[k][1],
            s_callers[k][2]));
}

static void __stdcall PopSeen(unsigned bits)
{
    s_popEmits++;
    char text[128];
    if (s_popEmits % 1000 == 0)
    {
        char* g = *(char**)0x008d5338;
        char* mixer = g ? *(char**)(g + 0x160) : 0;
        unsigned count = mixer ? *(unsigned*)(*(char**)(mixer + 0x280) + 0x106f8) : 0;
        unsigned limit = mixer ? *(unsigned*)(mixer + 0x440) : 0;
        AppendLog(text, wsprintfA(text, "sounds: emit %u frame %u live %u limit %u made %u refused %u\n", s_popEmits,
            s_ticFrames, count, limit, s_soundsMade, s_soundsRefused));
        if (s_popEmits == 12000)
            LogCallers();
        if (mixer && (s_popEmits == 5000 || s_popEmits == 9000))
        {
            // The live sounds: E32Walk::LastDown (down +0x28), then each next (+0x24); the name SetName copies to +4.
            char* p = *(char**)(mixer + 0x280);
            while (*(char**)(p + 0x28))
                p = *(char**)(p + 0x28);
            for (int k = 0; p && k < 40; k++, p = *(char**)(p + 0x24))
            {
                char name[32];
                memcpy(name, p + 4, 31);
                name[31] = 0;
                for (int c = 0; name[c]; c++)
                    if (name[c] < 32 || name[c] > 126)
                        name[c] = '?';
                AppendLog(text, wsprintfA(text, "sounds: live %08x vt %08x %s\n", (unsigned)p, *(unsigned*)p, name));
            }
        }
    }
    if (bits == 0)
        return;
    AppendLog(text, wsprintfA(text, "pop: emit %u misfire %08x\n", s_popEmits, bits));
}

static __declspec(naked) void MadeHook()
{
    __asm
    {
        inc s_soundsMade
        push 0x250
        mov eax, 0x004cf8d0
        jmp eax
    }
}

static __declspec(naked) void Made2Hook()
{
    __asm
    {
        inc s_soundsMade
        pushad
        push 0
        lea eax, [esp + 0x24]
        push eax
        call SoundBy
        popad
        push 0x250
        mov eax, 0x004cf9d0
        jmp eax
    }
}

static __declspec(naked) void Refused2Hook()
{
    __asm
    {
        inc s_soundsRefused
        pushad
        push 1
        lea eax, [esp + 0x24]
        push eax
        call SoundBy
        popad
        lea eax, [edi + 0x40]
        mov ecx, dword ptr [esp + 0xc]
        push 0x004cf97f
        ret
    }
}

static __declspec(naked) void RefusedHook()
{
    __asm
    {
        inc s_soundsRefused
        lea eax, [edi + 0x40]
        mov ecx, dword ptr [esp + 0xc]
        push 0x004cf87f
        ret
    }
}

static __declspec(naked) void PopHook()
{
    __asm
    {
        pushad
        push dword ptr [esp + 0x24]
        call PopSeen
        popad
        mov eax, 0x004af280
        jmp eax
    }
}

static void HookPops(FILE* log)
{
    s_popsOn = 1;
    const unsigned char* p = (const unsigned char*)kPopCall;
    if (p[0] != 0xE8 || *(int*)(p + 1) != (int)(0x004af280 - (kPopCall + 5)))
    {
        fprintf(log, "pops: call site differs\n");
        return;
    }
    unsigned char call[5] = { 0xE8, 0, 0, 0, 0 };
    *(int*)(call + 1) = (int)((unsigned)&PopHook - (kPopCall + 5));
    WriteCode((unsigned char*)kPopCall, call, 5);
    static const unsigned char made[5] = { 0x68, 0x50, 0x02, 0x00, 0x00 };
    static const unsigned char refused[7] = { 0x8D, 0x47, 0x40, 0x8B, 0x4C, 0x24, 0x0C };
    if (memcmp((void*)0x004cf8cb, made, 5) == 0 && memcmp((void*)0x004cf878, refused, 7) == 0)
    {
        unsigned char jmp[7] = { 0xE9, 0, 0, 0, 0, 0x90, 0x90 };
        *(int*)(jmp + 1) = (int)((unsigned)&MadeHook - (0x004cf8cb + 5));
        WriteCode((unsigned char*)0x004cf8cb, jmp, 5);
        *(int*)(jmp + 1) = (int)((unsigned)&RefusedHook - (0x004cf878 + 5));
        WriteCode((unsigned char*)0x004cf878, jmp, 7);
    }
    else
        fprintf(log, "pops: E30DXMixer::Vf4c differs\n");
    if (memcmp((void*)0x004cf9cb, made, 5) == 0 && memcmp((void*)0x004cf978, refused, 7) == 0)
    {
        unsigned char jmp[7] = { 0xE9, 0, 0, 0, 0, 0x90, 0x90 };
        *(int*)(jmp + 1) = (int)((unsigned)&Made2Hook - (0x004cf9cb + 5));
        WriteCode((unsigned char*)0x004cf9cb, jmp, 5);
        *(int*)(jmp + 1) = (int)((unsigned)&Refused2Hook - (0x004cf978 + 5));
        WriteCode((unsigned char*)0x004cf978, jmp, 7);
    }
    else
        fprintf(log, "pops: E30DXMixer::Vf48 differs\n");
    fprintf(log, "pops: logging\n");
}

// RBRRecompFps.txt (RBR_FPS=N): at most N frames a second (CApplication::Tic waits), standing for the retrace wait
// a windowed game under Wine does not get. s_ticFrames counts the frames for the pops log.

static void __stdcall FrameLimit()
{
    s_ticFrames++;
    if (!s_fpsLimit)
        return;
    static __int64 next, period;
    __int64 now;
    QueryPerformanceCounter(&now);
    if (!period)
    {
        __int64 frequency;
        QueryPerformanceFrequency(&frequency);
        period = frequency / s_fpsLimit;
        next = now;
    }
    next += period;
    if (now > next)
    {
        next = now;  // behind by more than a frame: start again from now
        return;
    }
    while (now < next)
    {
        if (next - now > period / 8)
            Sleep(1);
        QueryPerformanceCounter(&now);
    }
}

static void LoadFps(FILE* log)
{
    FILE* f = fopen("RBRRecompFps.txt", "r");
    if (!f)
        return;
    if (fscanf(f, "%u", &s_fpsLimit) != 1)
        s_fpsLimit = 0;
    fclose(f);
    fprintf(log, "fps: at most %u frames a second\n", s_fpsLimit);
}

static void VolumesTick()
{
    float* options = (float*)0x01660e08;
    if (!s_volumesLogged)
    {
        char text[160];
        AppendLog(text, sprintf(text, "volumes: the game's options %.3f %.3f %.3f %.3f %.3f %.3f\n", options[0], options[1],
            options[2], options[3], options[4], options[5]));
        s_volumesLogged = 1;
    }
    for (int k = 0; k < 6; k++)
        if (s_volumeSet[k])
            options[k] = s_volumes[k];
}

// With the pops log: each change of CCarSound's engine-stopped flag (m_data +0x41750, the car sound at the
// master's +0x105c4), which NGP's FMOD daemon pauses the engine events on, with the frame and the battery word.
static void EngineStoppedTick(int state)
{
    static int last = -2;
    char* master = *(char**)0x008d5338;
    char* car = master && !IsBadReadPtr(master + 0x105c4, 4) ? *(char**)(master + 0x105c4) : 0;
    if (!car || IsBadReadPtr(car + 0x41924, 4))
        return;
    int flag = *(int*)(car + 0x41924);
    if (flag == last)
        return;
    last = flag;
    char text[128];
    AppendLog(text, sprintf(text, "engine-stopped: frame %u state %d emit %u flag %d battery %.3f\n", s_ticFrames, state,
        s_popEmits, flag, *(float*)(car + 0x418f0)));
}

static void __stdcall CameraTick()
{
    void* app = *(void**)0x007eac48;
    int state = app ? *(int*)((char*)app + 0x728) : -1;
    if (s_popsOn)
        EngineStoppedTick(state);
    static int lastState = -2;
    if ((s_cameraMode >= 0 || s_volumesLogged) && state != lastState)
    {
        char text[64];
        AppendLog(text, wsprintfA(text, "app state %d\n", state));
    }
    lastState = state;
    if (state == 1 || state == 8 || state == 10)
        VolumesTick();
    if (s_cameraMode < 0 || !app || *(int*)((char*)app + 0x728) != 1)
        return;
    int* mode = ((int* (__fastcall*)(void*))0x0047ca00)(app);
    int was = *mode;
    if (was == s_cameraMode)
        return;
    __asm
    {
        push 0
        mov ecx, mode
        mov eax, 0x00487840
        call eax
    }
    if (*mode != was)
    {
        char text[64];
        AppendLog(text, wsprintfA(text, "camera: stepped from mode %d to %d\n", was, *mode));
    }
}

static __declspec(naked) void TicHook()
{
    __asm
    {
        pushad
        call FrameLimit
        call WatchGameFrame
        call CameraTick
        call AutoStart
        popad
        push -1
        push 0x00712358
        push 0x004804f7
        ret
    }
}

static void StartAuto(FILE* log, IRBRGame* game)
{
    LoadCamera(log);
    LoadVolumes(log);
    LoadFps(log);
    if (FILE* pops = fopen("RBRRecompPops.txt", "r"))
    {
        fclose(pops);
        HookPops(log);
    }
    int n = 0;
    FILE* f = fopen("RBRRecompReplay.txt", "r");
    if (f)
    {
        char path[300];
        n = fscanf(f, "%259[^\r\n]", s_replay) == 1 ? 4 : 0;
        fclose(f);
        wsprintfA(path, "Replays\\%s", s_replay);
        FILE* r = n ? fopen(path, "rb") : 0;
        if (r)
        {
            fseek(r, 0, SEEK_END);
            s_replaySize = (unsigned)ftell(r);
            fclose(r);
        }
        else
            n = 0;
    }
    else
    {
        f = fopen("RBRRecompAuto.txt", "r");
        if (!f)
            return;
        s_auto[4] = s_auto[5] = -1;
        n = fscanf(f, "%d %d %d %d %d %d", &s_auto[0], &s_auto[1], &s_auto[2], &s_auto[3], &s_auto[4], &s_auto[5]);
        fclose(f);
    }
    if (n < 4 || memcmp((void*)kTic, kTicPrologue, 7) != 0)
    {
        fprintf(log, "auto: bad RBRRecompAuto.txt or RBRRecompReplay.txt, or Tic prologue differs\n");
        return;
    }
    s_game = game;
    s_autoNext = GetTickCount() + 500;
    unsigned char jmp[7] = { 0xE9, 0, 0, 0, 0, 0x90, 0x90 };
    *(unsigned*)(jmp + 1) = (unsigned)&TicHook - (kTic + 5);
    WriteCode((unsigned char*)kTic, jmp, 7);
    if (s_replay[0])
        fprintf(log, "auto: replay %s (%u bytes)\n", s_replay, s_replaySize);
    else
        fprintf(log, "auto: map %d car %d weather %d tyre %d damage %d transmission %d\n", s_auto[0], s_auto[1], s_auto[2], s_auto[3], s_auto[4], s_auto[5]);
}

class CHarnessPlugin : public IPlugin
{
public:
    const char* GetName() { return "rbr-recomp harness"; }
    void DrawFrontEndPage() {}
    void DrawResultsUI() {}
    void HandleFrontEndEvents(char, bool, bool, bool, bool, bool) {}
    void TickFrontEndPage(float) {}
    void StageStarted(int map, const char*, bool)
    {
        // what RBRRecompAuto.txt needs to reproduce this run (car index slot 0x01660808)
        char path[320];
        FILE* log = fopen(GameFile(path, "RBRRecompHarness.log"), "a");
        if (log)
        {
            fprintf(log, "stage started: map %d car %d damage %d transmission %d\n", map, *(int*)0x01660808, *(int*)kDamage, *(int*)kTransmission);
            if (s_rec || s_play)
                HookCarControl(log);
            fclose(log);
        }
        if (s_writeWatch)
        {
            s_watchFrame = 0;
            s_watchStageStarted = 1;
        }
    }
    void HandleResults(float, float, float, const char*) {}
    void CheckPoint(float, int, const char*) {}
};

extern "C" __declspec(dllexport) IPlugin* RBR_CreatePlugin(IRBRGame* game)
{
    GetCurrentDirectoryA(sizeof(s_gameDir), s_gameDir);
    FILE* log = fopen("RBRRecompHarness.log", "w");
    if (log)
    {
        // Playback drives the car by itself: leave the FFB fix out so the wheel stays
        // still (unpatched, Wine rejects every force update).
        FILE* play = fopen("RBRRecompPlayback.bin", "rb");
        if (play)
        {
            fclose(play);
            fprintf(log, "playback: force feedback left off\n");
        }
        else
            ApplyPatches(log);
        StartAllocLog(log);
        ApplyTransplant(log);
        StartWriteWatch(log);
        StartFocusHandling(log);
        StartRecorder(log);
        StartAuto(log, game);
        fclose(log);
    }
    return new CHarnessPlugin;
}
