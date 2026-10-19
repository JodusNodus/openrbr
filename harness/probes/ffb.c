/* Constant-force probe (no CRT), shaped like RBR: one cartesian constant force on the
   force-feedback axes of the first driving device; left 2 s, right 2 s. */
#define DIRECTINPUT_VERSION 0x0800
#define COBJMACROS
#include <windows.h>
#include <dinput.h>

static HANDLE out;
static IDirectInput8A* di;
static IDirectInputDevice8A* dev;
static DWORD axes[2]; static int naxes;
static void say(const char* fmt, ...)
{
    char b[512]; DWORD n; va_list a;
    va_start(a, fmt); n = wvsprintfA(b, fmt, a); va_end(a);
    WriteFile(out, b, n, &n, 0);
}
static BOOL CALLBACK DevCb(LPCDIDEVICEINSTANCEA d, void* u)
{
    if (dev == 0 && SUCCEEDED(IDirectInput8_CreateDevice(di, &d->guidInstance, &dev, 0)))
        say("device '%s'\r\n", d->tszProductName);
    return DIENUM_CONTINUE;
}
static BOOL CALLBACK AxCb(LPCDIDEVICEOBJECTINSTANCEA o, void* u)
{
    if ((o->dwFlags & DIDOI_FFACTUATOR) && naxes < 2) { axes[naxes++] = o->dwOfs; say("ff axis '%s' ofs %lu\r\n", o->tszName, o->dwOfs); }
    return DIENUM_CONTINUE;
}
void __stdcall start(void)
{
    HWND w; HRESULT hr; IDirectInputEffect* eff = 0; DIEFFECT e; DICONSTANTFORCE cf; LONG dir[2] = {1, 0}; int i;
    out = CreateFileA("ffb.txt", GENERIC_WRITE, FILE_SHARE_READ, 0, CREATE_ALWAYS, 0, 0);
    w = CreateWindowExA(0, "STATIC", "fftest", WS_OVERLAPPEDWINDOW | WS_VISIBLE, 0, 0, 320, 120, 0, 0, 0, 0);
    SetForegroundWindow(w);
    hr = DirectInput8Create(GetModuleHandleA(0), DIRECTINPUT_VERSION, &IID_IDirectInput8A, (void**)&di, 0);
    IDirectInput8_EnumDevices(di, DI8DEVCLASS_GAMECTRL, DevCb, 0, DIEDFL_ATTACHEDONLY | DIEDFL_FORCEFEEDBACK);
    if (!dev) { say("no FF device\r\n"); ExitProcess(1); }
    IDirectInputDevice8_SetDataFormat(dev, &c_dfDIJoystick2);
    hr = IDirectInputDevice8_SetCooperativeLevel(dev, w, DISCL_EXCLUSIVE | DISCL_BACKGROUND); say("coop hr %08lx\r\n", hr);
    hr = IDirectInputDevice8_Acquire(dev); say("acquire hr %08lx\r\n", hr);
    IDirectInputDevice8_EnumObjects(dev, AxCb, 0, DIDFT_AXIS);
    cf.lMagnitude = 6000;
    { volatile char* p = (volatile char*)&e; unsigned k; for (k = 0; k < sizeof(e); k++) p[k] = 0; }
    e.dwSize = sizeof(e); e.dwFlags = DIEFF_CARTESIAN | DIEFF_OBJECTOFFSETS; e.dwDuration = INFINITE;
    e.dwGain = DI_FFNOMINALMAX; e.dwTriggerButton = DIEB_NOTRIGGER; e.cAxes = naxes; e.rgdwAxes = axes;
    e.rglDirection = dir; e.cbTypeSpecificParams = sizeof(cf); e.lpvTypeSpecificParams = &cf;
    hr = IDirectInputDevice8_CreateEffect(dev, &GUID_ConstantForce, &e, &eff, 0); say("create hr %08lx\r\n", hr);
    if (eff) {
        hr = IDirectInputEffect_Start(eff, 1, 0); say("start hr %08lx\r\n", hr);
        for (i = 0; i < 40; i++) {
            cf.lMagnitude = i < 20 ? 6000 : -6000;
            hr = IDirectInputEffect_SetParameters(eff, &e, DIEP_TYPESPECIFICPARAMS | DIEP_START);
            if (i == 0 || i == 20) say("set hr %08lx (magnitude %ld)\r\n", hr, cf.lMagnitude);
            Sleep(100);
        }
        IDirectInputEffect_Stop(eff);
        IDirectInputEffect_Release(eff);
    }
    IDirectInputDevice8_Unacquire(dev);
    CloseHandle(out);
    ExitProcess(0);
}
