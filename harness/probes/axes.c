/* DirectInput axis probe (no CRT): what RBR's E29EnumObjectsCb sees per controller. */
#define DIRECTINPUT_VERSION 0x0800
#define COBJMACROS
#include <windows.h>
#include <dinput.h>

static HANDLE out;
static IDirectInput8A* di;
static void say(const char* fmt, ...)
{
    char b[512]; DWORD n; va_list a;
    va_start(a, fmt); n = wvsprintfA(b, fmt, a); va_end(a);
    WriteFile(out, b, n, &n, 0);
}

static BOOL CALLBACK ObjCb(LPCDIDEVICEOBJECTINSTANCEA o, void* ctx)
{
    IDirectInputDevice8A* dev = (IDirectInputDevice8A*)ctx;
    DIPROPRANGE r, r2; HRESULT hr, hr2;
    r.diph.dwSize = sizeof(r); r.diph.dwHeaderSize = sizeof(DIPROPHEADER);
    r.diph.dwHow = DIPH_BYID; r.diph.dwObj = o->dwType; r.lMin = r.lMax = 0;
    hr = IDirectInputDevice8_GetProperty(dev, DIPROP_RANGE, &r.diph);
    r2 = r; r2.diph.dwObj = o->dwType & 0x00ffffff; r2.lMin = r2.lMax = 0;
    hr2 = IDirectInputDevice8_GetProperty(dev, DIPROP_RANGE, &r2.diph);
    say("  axis '%s' type %08lx flags %lx ofs %lu: byid(type) hr %08lx [%ld,%ld]  byid(no ff bits) hr %08lx [%ld,%ld]\r\n",
        o->tszName, o->dwType, o->dwFlags, o->dwOfs, hr, r.lMin, r.lMax, hr2, r2.lMin, r2.lMax);
    return DIENUM_CONTINUE;
}

static BOOL CALLBACK DevCb(LPCDIDEVICEINSTANCEA d, void* unused)
{
    IDirectInputDevice8A* dev = 0; int i;
    say("device '%s' type %08lx\r\n", d->tszProductName, d->dwDevType);
    if (FAILED(IDirectInput8_CreateDevice(di, &d->guidInstance, &dev, 0))) return DIENUM_CONTINUE;
    IDirectInputDevice8_EnumObjects(dev, ObjCb, dev, DIDFT_AXIS);
    IDirectInputDevice8_SetDataFormat(dev, &c_dfDIJoystick2);
    IDirectInputDevice8_SetCooperativeLevel(dev, GetDesktopWindow(), DISCL_BACKGROUND | DISCL_NONEXCLUSIVE);
    IDirectInputDevice8_Acquire(dev);
    for (i = 0; i < 20; i++) {
        DIJOYSTATE2 s;
        IDirectInputDevice8_Poll(dev);
        if (SUCCEEDED(IDirectInputDevice8_GetDeviceState(dev, sizeof(s), &s)))
            say("  t%d X %ld Y %ld Z %ld Rz %ld S0 %ld\r\n", i, s.lX, s.lY, s.lZ, s.lRz, s.rglSlider[0]);
        Sleep(500);
    }
    IDirectInputDevice8_Unacquire(dev);
    IDirectInputDevice8_Release(dev);
    return DIENUM_CONTINUE;
}

void __stdcall start(void)
{
    out = CreateFileA("axes.txt", GENERIC_WRITE, FILE_SHARE_READ, 0, CREATE_ALWAYS, 0, 0);
    if (SUCCEEDED(DirectInput8Create(GetModuleHandleA(0), DIRECTINPUT_VERSION, &IID_IDirectInput8A, (void**)&di, 0)))
        IDirectInput8_EnumDevices(di, DI8DEVCLASS_GAMECTRL, DevCb, 0, DIEDFL_ATTACHEDONLY);
    CloseHandle(out);
    ExitProcess(0);
}
