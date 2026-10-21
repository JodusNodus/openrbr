/* NGP's force-feedback calls replayed through DirectInput (no CRT, no dinput.h; MSVC 7.1, harness/ffb_trace.sh
   probe): the first force-feedback game controller, exclusive and background; one constant force created as the
   exe's CreateFFB 0x4c8c60 makes it (DIEFF_CARTESIAN | DIEFF_OBJECTIDS, every FF axis, gain 10000, infinite, all
   directions 0); then for each line `seconds magnitude` of ffb_replay.txt, at that time, the call RSF's NGP.dll
   7.5.779 makes (RSS1 EFF1 of drive2): SetParameters(DIEP_START | DIEP_TYPESPECIFICPARAMS | DIEP_DIRECTION |
   DIEP_DURATION = 0x20000141) with DIEFF_CARTESIAN | DIEFF_OBJECTOFFSETS, two axes, direction {1, 0}, duration
   INFINITE. Writes ffb_replay.log. */
typedef unsigned long DWORD;
typedef long LONG, HRESULT;
typedef unsigned short WORD;
typedef unsigned char BYTE;
typedef int BOOL;
typedef void *HANDLE, *HWND, *HINSTANCE;
typedef __int64 LONGLONG;
typedef struct { DWORD Data1; WORD Data2, Data3; BYTE Data4[8]; } GUID;
#define MAX_PATH 260
#define INFINITE 0xffffffff
#define TRUE 1
__declspec(dllimport) HANDLE __stdcall CreateFileA(const char*, DWORD, DWORD, void*, DWORD, DWORD, HANDLE);
__declspec(dllimport) BOOL __stdcall ReadFile(HANDLE, void*, DWORD, DWORD*, void*);
__declspec(dllimport) BOOL __stdcall WriteFile(HANDLE, const void*, DWORD, DWORD*, void*);
__declspec(dllimport) BOOL __stdcall CloseHandle(HANDLE);
__declspec(dllimport) void __stdcall ExitProcess(unsigned);
__declspec(dllimport) void __stdcall Sleep(DWORD);
__declspec(dllimport) BOOL __stdcall QueryPerformanceCounter(LONGLONG*);
__declspec(dllimport) BOOL __stdcall QueryPerformanceFrequency(LONGLONG*);
__declspec(dllimport) HINSTANCE __stdcall GetModuleHandleA(const char*);
__declspec(dllimport) HINSTANCE __stdcall LoadLibraryA(const char*);
__declspec(dllimport) void* __stdcall GetProcAddress(HINSTANCE, const char*);
__declspec(dllimport) HWND __stdcall CreateWindowExA(DWORD, const char*, const char*, DWORD, int, int, int, int, HWND, void*,
    HINSTANCE, void*);
__declspec(dllimport) int __cdecl wsprintfA(char*, const char*, ...);
#define GENERIC_READ 0x80000000
#define GENERIC_WRITE 0x40000000
#define FILE_SHARE_READ 1
#define CREATE_ALWAYS 2
#define OPEN_EXISTING 3
#define WS_VISIBLE_OVERLAPPED 0x10cf0000
#define INVALID_HANDLE_VALUE ((HANDLE)-1)

typedef struct { DWORD dwSize; GUID guidInstance, guidProduct; DWORD dwDevType; char tszInstanceName[MAX_PATH],
    tszProductName[MAX_PATH]; GUID guidFFDriver; WORD wUsagePage, wUsage; } DevInst;
typedef struct { DWORD dwSize; GUID guidType; DWORD dwOfs, dwType, dwFlags; char tszName[MAX_PATH];
    DWORD dwFFMaxForce, dwFFForceResolution; WORD wCollectionNumber, wDesignatorIndex, wUsagePage, wUsage;
    DWORD dwDimension; WORD wExponent, wReportId; } ObjInst;
typedef struct { DWORD dwSize, dwFlags, dwDuration, dwSamplePeriod, dwGain, dwTriggerButton,
    dwTriggerRepeatInterval, cAxes; DWORD* rgdwAxes; LONG* rglDirection; void* lpEnvelope;
    DWORD cbTypeSpecificParams; void* lpvTypeSpecificParams; DWORD dwStartDelay; } Effect;
typedef void* Obj;  /* a COM object: its first word is the vtable */
#define METHOD(o, i, sig) ((sig)((*(void***)(o))[i]))
typedef HRESULT (__stdcall *CreateDeviceFn)(Obj, const GUID*, Obj*, void*);
typedef HRESULT (__stdcall *EnumDevicesFn)(Obj, DWORD, BOOL (__stdcall*)(const DevInst*, void*), void*, DWORD);
typedef HRESULT (__stdcall *EnumObjectsFn)(Obj, BOOL (__stdcall*)(const ObjInst*, void*), void*, DWORD);
typedef HRESULT (__stdcall *ObjFn)(Obj);
typedef HRESULT (__stdcall *PtrFn)(Obj, const void*);
typedef HRESULT (__stdcall *CoopFn)(Obj, HWND, DWORD);
typedef HRESULT (__stdcall *CreateEffectFn)(Obj, const GUID*, const Effect*, Obj*, void*);
typedef HRESULT (__stdcall *SetParametersFn)(Obj, const Effect*, DWORD);
typedef HRESULT (__stdcall *DirectInput8CreateFn)(HINSTANCE, DWORD, const GUID*, Obj*, void*);

static const GUID IID_IDirectInput8A = {0xbf798030, 0x483a, 0x4da2, {0xaa, 0x99, 0x5d, 0x64, 0xed, 0x36, 0x97, 0x00}};
static const GUID GUID_ConstantForce = {0x13541c20, 0x8e33, 0x11d0, {0x9a, 0xd0, 0x00, 0xa0, 0xc9, 0xa0, 0x6e, 0x35}};
extern const BYTE c_dfDIJoystick2[];  /* dinput8.lib */

static HANDLE out;
static Obj di, dev;
static DWORD axes[2];
static DWORD naxes;

static char line[512];
static void say(int n)
{
    DWORD done;
    WriteFile(out, line, n, &done, 0);
}
#define SAY(args) say(wsprintfA args)

static BOOL __stdcall DevCb(const DevInst* d, void* u)
{
    if (!dev && METHOD(di, 3, CreateDeviceFn)(di, &d->guidInstance, &dev, 0) >= 0)
        SAY((line, "device '%s'\r\n", d->tszProductName));
    return TRUE;
}

static BOOL __stdcall AxCb(const ObjInst* o, void* u)
{
    if ((o->dwFlags & 1 /* DIDOI_FFACTUATOR */) && naxes < 2) axes[naxes++] = o->dwType;
    return TRUE;
}

void __stdcall start(void)
{
    static char text[1 << 22];
    HANDLE in; DWORD size, i; HWND w; HRESULT hr; Obj eff = 0; Effect e; LONG cf, dir[2] = {0, 0}, dir_set[2] = {1, 0};
    LONGLONG freq, t0, now; long n = 0, failed = 0, skipped = 0;
    DirectInput8CreateFn create = (DirectInput8CreateFn)GetProcAddress(LoadLibraryA("dinput8.dll"), "DirectInput8Create");
    out = CreateFileA("ffb_replay.log", GENERIC_WRITE, FILE_SHARE_READ, 0, CREATE_ALWAYS, 0, 0);
    in = CreateFileA("ffb_replay.txt", GENERIC_READ, FILE_SHARE_READ, 0, OPEN_EXISTING, 0, 0);
    if (in == INVALID_HANDLE_VALUE || !ReadFile(in, text, sizeof text - 1, &size, 0)) { SAY((line, "no ffb_replay.txt\r\n")); ExitProcess(1); }
    text[size] = 0;
    w = CreateWindowExA(0, "STATIC", "ffb_replay", WS_VISIBLE_OVERLAPPED, 0, 0, 320, 120, 0, 0, 0, 0);
    hr = create(GetModuleHandleA(0), 0x0800, &IID_IDirectInput8A, &di, 0);
    METHOD(di, 4, EnumDevicesFn)(di, 4 /* DI8DEVCLASS_GAMECTRL */, DevCb, 0, 0x101 /* ATTACHEDONLY | FORCEFEEDBACK */);
    if (!dev) { SAY((line, "no FF device\r\n")); ExitProcess(1); }
    METHOD(dev, 11, PtrFn)(dev, c_dfDIJoystick2);
    hr = METHOD(dev, 13, CoopFn)(dev, w, 1 | 8 /* EXCLUSIVE | BACKGROUND */); SAY((line, "coop %08lx\r\n", hr));
    hr = METHOD(dev, 7, ObjFn)(dev); SAY((line, "acquire %08lx\r\n", hr));
    METHOD(dev, 4, EnumObjectsFn)(dev, AxCb, 0, 3 /* DIDFT_AXIS */);
    cf = 0;
    for (i = 0; i < sizeof e; i++) ((volatile char*)&e)[i] = 0;
    e.dwSize = sizeof e; e.dwFlags = 0x11; e.dwDuration = INFINITE; e.dwGain = 10000; e.dwTriggerButton = INFINITE;
    e.cAxes = naxes; e.rgdwAxes = axes; e.rglDirection = dir; e.cbTypeSpecificParams = 4; e.lpvTypeSpecificParams = &cf;
    hr = METHOD(dev, 18, CreateEffectFn)(dev, &GUID_ConstantForce, &e, &eff, 0);
    SAY((line, "%lu axes %08lx %08lx, create %08lx\r\n", naxes, axes[0], axes[1], hr));
    if (!eff) ExitProcess(1);
    e.dwFlags = 0x12; e.rgdwAxes = 0; e.rglDirection = dir_set; e.cAxes = 2; e.dwGain = 0; e.dwTriggerButton = 0;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&t0);
    for (i = 0; text[i];) {
        LONGLONG us = 0, frac = 100000; LONG m = 0, sign = 1; DWORD at, digits;
        while (text[i] == ' ' || text[i] == '\r' || text[i] == '\n') i++;
        if (!text[i]) break;
        at = i;
        while (text[i] >= '0' && text[i] <= '9') us = us * 10 + (text[i++] - '0');
        us *= 1000000;
        if (text[i] == '.') for (i++; text[i] >= '0' && text[i] <= '9'; i++, frac /= 10) us += (text[i] - '0') * frac;
        while (text[i] == ' ') i++;
        if (text[i] == '-') { sign = -1; i++; }
        digits = i;
        while (text[i] >= '0' && text[i] <= '9') m = m * 10 + (text[i++] - '0');
        while (text[i] == ' ') i++;
        /* a line that is not `seconds magnitude` is skipped whole: never sent, never looped on */
        if (i == at || i == digits || (text[i] && text[i] != '\r' && text[i] != '\n')) {
            while (text[i] && text[i] != '\n') i++;
            skipped++;
            continue;
        }
        cf = sign * m;
        for (;;) {
            LONGLONG left;
            QueryPerformanceCounter(&now);
            left = us - (now - t0) * 1000000 / freq;
            if (left <= 0) break;
            Sleep(left > 1500 ? 1 : 0);
        }
        hr = METHOD(eff, 6, SetParametersFn)(eff, &e, 0x20000141);
        n++;
        if (hr < 0) failed++;
    }
    cf = 0;
    METHOD(eff, 6, SetParametersFn)(eff, &e, 0x20000141);
    SAY((line, "%ld calls, %ld failed, %ld malformed lines skipped\r\n", n, failed, skipped));
    METHOD(eff, 8, ObjFn)(eff);
    METHOD(eff, 2, ObjFn)(eff);
    METHOD(dev, 8, ObjFn)(dev);
    CloseHandle(out);
    ExitProcess(0);
}
