/* OPEN1 (AUDIT/memory.md §6.4): WebKit's helper processes trim themselves.
 *
 * Built as libarcade-trim.so and listed in LD_PRELOAD by the AppRun hook of
 * the WebKit apps (Tools, Box, Look) only for WebKit's helper processes
 * (WebKitWebProcess, WebKitNetworkProcess). In those it starts one thread
 * that runs arcade_trim() 2 s after start and then every 20 s. It does
 * nothing in any other process, so children the app starts are unaffected.
 *
 *   cc -O2 -shared -fPIC -o libarcade-trim.so arcade-trim-preload.c -I<link>/qt -lpthread
 */
#define _GNU_SOURCE
#include "arcade_trim.h"
#include <errno.h>
#include <pthread.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static void sleep_seconds(time_t s)
{
    struct timespec t = {s, 0};
    while (nanosleep(&t, &t) == -1 && errno == EINTR) {
    }
}

static void *trimmer(void *unused)
{
    (void)unused;
    sleep_seconds(2);
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
