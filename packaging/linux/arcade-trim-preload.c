/* OPEN1 (AUDIT/memory.md §6.4): WebKit's helper processes trim themselves.
 *
 * Built as libarcade-trim.so and listed in LD_PRELOAD by the AppRun hook of
 * the WebKit apps (Tools, Box, Look) only for WebKit's helper processes
 * (WebKitWebProcess, WebKitNetworkProcess). In those it starts one thread
 * that runs arcade_trim() 2 s after start, again each time the process
 * settles during the next 15 s, and then every 20 s. It does nothing in any
 * other process, so children the app starts are unaffected.
 *
 * The trim at 2 s alone left two levels of memory. On a slow or loaded
 * machine the page is still loading at 2 s; the code it runs afterwards is
 * read back in and stays resident until the trim at 22 s. Six seconds after
 * its window opened, the same Tools build read 250 MB PSS on most CI runners
 * and 290 to 310 on a slow one, 48 MB of it library code in the web process.
 * Trimming again once the page faults stop gives one level on both.
 *
 *   cc -O2 -shared -fPIC -o libarcade-trim.so arcade-trim-preload.c -I<link>/qt -lpthread
 */
#define _GNU_SOURCE
#include "arcade_trim.h"
#include <errno.h>
#include <pthread.h>
#include <string.h>
#include <sys/resource.h>
#include <time.h>
#include <unistd.h>

static void sleep_seconds(time_t s)
{
    struct timespec t = {s, 0};
    while (nanosleep(&t, &t) == -1 && errno == EINTR) {
    }
}

#define SETTLE_POLL_MS 500
#define SETTLE_SECONDS 15
/* Active: more page faults in one poll than this. Reading back what a trim
 * dropped takes hundreds to thousands; an open page at rest takes none or a
 * few dozen. */
#define SETTLE_FAULTS 64

static long faults(void)
{
    struct rusage u;
    return getrusage(RUSAGE_SELF, &u) == 0 ? u.ru_minflt + u.ru_majflt : 0;
}

/* For SETTLE_SECONDS: a trim each time two quiet polls follow an active one
 * (the load, then the first paint when the window is shown). Two wakeups a
 * second while it lasts, none after. */
static void trim_as_it_settles(void)
{
    long last = faults();
    int active = 0, quiet = 0;
    for (int i = 0; i < SETTLE_SECONDS * 1000 / SETTLE_POLL_MS; i++) {
        struct timespec t = {0, SETTLE_POLL_MS * 1000000L};
        while (nanosleep(&t, &t) == -1 && errno == EINTR) {
        }
        long now = faults();
        if (now - last > SETTLE_FAULTS) {
            active = 1;
            quiet = 0;
        } else if (++quiet >= 2 && active) {
            arcade_trim();
            active = 0;
            now = faults(); /* the trim's own faults are not the page's */
        }
        last = now;
    }
}

static void *trimmer(void *unused)
{
    (void)unused;
    sleep_seconds(2);
    arcade_trim();
    trim_as_it_settles();
    for (;;) {
        arcade_trim();
        sleep_seconds(20);
    }
    return NULL;
}

static int is_webkit_helper(void)
{
    extern char *program_invocation_short_name;
    return strncmp(program_invocation_short_name, "WebKitWebProces", 15) == 0
        || strncmp(program_invocation_short_name, "WebKitNetworkPr", 15) == 0;
}

__attribute__((constructor)) static void arcade_trim_start(void)
{
    if (!is_webkit_helper())
        return;
    pthread_attr_t attr;
    pthread_t thread;
    pthread_attr_init(&attr);
    pthread_attr_setstacksize(&attr, 64 * 1024);
    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    pthread_create(&thread, &attr, trimmer, NULL);
    pthread_attr_destroy(&attr);
}
