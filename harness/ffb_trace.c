/* ffb_trace.so: an LD_PRELOAD library that logs what a Linux process sends to its input devices' force feedback,
   without ptrace or root (harness/ffb_trace.py builds, runs and reads it). For each EVIOCSFF and EVIOCRMFF ioctl and
   each EV_FF event written to a /dev/input/event* file, and each write to a /dev/hidraw* file, one line in
   $FFB_TRACE_DIR/ffb-<pid>.txt:

     <CLOCK_MONOTONIC ns> <thread id> <device path> sff <ret> id type dir length delay level attack_length
         attack_level fade_length fade_level        (FF_CONSTANT; other types: the 8 u16 words of the union)
     ... rmff <ret> id
     ... ev <ret> code value                        (EV_FF: code is the effect id, FF_GAIN or FF_AUTOCENTER)
     ... hid <ret> bytes-in-hex

   It forwards every call through syscall() and needs no newer glibc symbol than 2.17, so it loads in Proton's
   container runtime too. Build: cc -O2 -shared -fPIC -fno-stack-protector -U_FORTIFY_SOURCE -o ffb_trace.so
   ffb_trace.c */
#define _GNU_SOURCE
#include <fcntl.h>
#include <linux/input.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

#define MAX_FD 4096
/* per fd: 0 not looked at, 1 event, 2 hidraw, 3 anything else; the path of 1 and 2 */
static unsigned char kinds[MAX_FD];
static char paths[MAX_FD][64];
static int out = -1;

static int kind(int fd)
{
    char link[32], target[64];
    long n;
    if (fd < 0 || fd >= MAX_FD) return 3;
    if (kinds[fd]) return kinds[fd];
    snprintf(link, sizeof link, "/proc/self/fd/%d", fd);
    n = syscall(SYS_readlinkat, AT_FDCWD, link, target, sizeof target - 1);
    if (n <= 0) return 3;
    target[n] = 0;
    kinds[fd] = !strncmp(target, "/dev/input/event", 16) ? 1 : !strncmp(target, "/dev/hidraw", 11) ? 2 : 3;
    if (kinds[fd] != 3) snprintf(paths[fd], sizeof paths[fd], "%s", target);
    return kinds[fd];
}

static void say(int fd, const char *fmt, ...)
{
    char line[512];
    struct timespec t;
    va_list a;
    int n;
    if (out < 0) {
        const char *dir = getenv("FFB_TRACE_DIR");
        char name[256];
        if (!dir) return;
        snprintf(name, sizeof name, "%s/ffb-%ld.txt", dir, (long)syscall(SYS_getpid));
        out = (int)syscall(SYS_openat, AT_FDCWD, name, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0644);
        if (out < 0) return;
    }
    syscall(SYS_clock_gettime, CLOCK_MONOTONIC, &t);
    n = snprintf(line, sizeof line, "%lld %ld %s ", (long long)t.tv_sec * 1000000000LL + t.tv_nsec,
                 (long)syscall(SYS_gettid), paths[fd]);
    va_start(a, fmt);
    n += vsnprintf(line + n, sizeof line - n - 1, fmt, a);
    va_end(a);
    if (n > (int)sizeof line - 2) n = sizeof line - 2;
    line[n++] = '\n';
    syscall(SYS_write, out, line, n);
}

int ioctl(int fd, unsigned long request, ...)
{
    va_list a;
    void *arg;
    long r;
    va_start(a, request);
    arg = va_arg(a, void *);
    va_end(a);
    r = syscall(SYS_ioctl, fd, request, arg);
    if ((request == EVIOCSFF || request == EVIOCRMFF) && kind(fd) == 1) {
        if (request == EVIOCRMFF) say(fd, "rmff %ld %ld", r, (long)arg);
        else {
            const struct ff_effect *e = arg;
            if (e->type == FF_CONSTANT)
                say(fd, "sff %ld %d %u %u %u %u %d %u %u %u %u", r, e->id, e->type, e->direction, e->replay.length,
                    e->replay.delay, e->u.constant.level, e->u.constant.envelope.attack_length,
                    e->u.constant.envelope.attack_level, e->u.constant.envelope.fade_length,
                    e->u.constant.envelope.fade_level);
            else {
                const unsigned short *w = (const unsigned short *)&e->u;
                say(fd, "sff %ld %d %u %u %u %u %u %u %u %u %u %u %u %u", r, e->id, e->type, e->direction,
                    e->replay.length, e->replay.delay, w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]);
            }
        }
    }
    return (int)r;
}

ssize_t write(int fd, const void *buf, size_t count)
{
    long r = syscall(SYS_write, fd, buf, count);
    int k = fd == out ? 3 : kind(fd);
    if (k == 1) {
        const struct input_event *e = buf;
        size_t i;
        for (i = 0; i < count / sizeof *e; ++i)
            if (e[i].type == EV_FF) say(fd, "ev %ld %u %d", r, e[i].code, e[i].value);
    } else if (k == 2) {
        char hex[2 * 64 + 1];
        size_t i, n = count < 64 ? count : 64;
        for (i = 0; i < n; ++i) snprintf(hex + 2 * i, 3, "%02x", ((const unsigned char *)buf)[i]);
        hex[2 * n] = 0;
        say(fd, "hid %ld %s", r, hex);
    }
    return r;
}

int close(int fd)
{
    if (fd >= 0 && fd < MAX_FD) kinds[fd] = 0;
    return (int)syscall(SYS_close, fd);
}
