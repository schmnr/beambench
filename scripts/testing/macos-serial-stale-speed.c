// Test-only macOS driver fault injection. Never loaded by Beam Bench itself.
// The first tcgetattr creates the pseudo terminal; the second opens it through
// RealSerialTransport. Emulate an IOSSIOSPEED value that tcsetattr rejects.
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <termios.h>

static unsigned reads;
static unsigned rejected;
static unsigned injected;
static const speed_t stale_speed = 123457;

static int stale_tcgetattr(int fd, struct termios *settings) {
    int result = tcgetattr(fd, settings);
    if (result == 0 && ++reads == 2) {
        settings->c_ispeed = stale_speed;
        settings->c_ospeed = stale_speed;
        ++injected;
    }
    return result;
}

static int rejecting_tcsetattr(int fd, int action, const struct termios *settings) {
    if (settings->c_ispeed == stale_speed || settings->c_ospeed == stale_speed) {
        ++rejected;
        errno = EINVAL;
        return -1;
    }
    return tcsetattr(fd, action, settings);
}

__attribute__((destructor)) static void report_injection(void) {
    fprintf(stderr, "stale-speed injection: injected=%u rejected=%u\n", injected, rejected);
}

__attribute__((used, section("__DATA,__interpose")))
static const struct { const void *replacement; const void *original; } hooks[] = {
    { (const void *)stale_tcgetattr, (const void *)tcgetattr },
    { (const void *)rejecting_tcsetattr, (const void *)tcsetattr },
};
