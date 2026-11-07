/* Arcade runtime memory trim, the C twin of arcade_link::runtime::trim (RT-5,
 * AUDIT/memory.md W0). Header-only; Qt and Flutter apps call
 * arcade_trim() after startup and after each hide.
 *
 * Gives freed heap back (malloc_trim) and drops resident pages of this
 * process's executable file mappings (r-xp only: read-only data and RELRO
 * would lose their relocations). Code refaults from the page cache.
 * Linux only; elsewhere it does nothing. */
#ifndef ARCADE_TRIM_H
#define ARCADE_TRIM_H

#if defined(__linux__)
#include <malloc.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>

static inline void arcade_trim(void)
{
    malloc_trim(0);
    FILE *maps = fopen("/proc/self/maps", "re");
    if (!maps)
        return;
    char line[4096];
    while (fgets(line, sizeof line, maps)) {
        unsigned long start = 0, end = 0;
        char perms[8] = "";
        if (sscanf(line, "%lx-%lx %7s", &start, &end, perms) != 3 || strncmp(perms, "r-xp", 4) != 0)
            continue;
        const char *path = strchr(line, '/');
        if (!path || strstr(line, "(deleted)"))
            continue;
        madvise((void *)start, end - start, MADV_DONTNEED);
    }
    fclose(maps);
}

/* W3b: page out private anonymous memory while the app is hidden. */
static inline void arcade_page_out_hidden(void)
{
#ifndef MADV_PAGEOUT
#define MADV_PAGEOUT 21
#endif
    FILE *maps = fopen("/proc/self/maps", "re");
    if (!maps)
        return;
    char line[4096];
    while (fgets(line, sizeof line, maps)) {
        unsigned long start = 0, end = 0;
        char perms[8] = "";
        if (sscanf(line, "%lx-%lx %7s", &start, &end, perms) != 3 || strncmp(perms, "rw-p", 4) != 0)
            continue;
        if (strchr(line, '/') || strchr(line, '['))
            continue;
        madvise((void *)start, end - start, MADV_PAGEOUT);
    }
    fclose(maps);
}
#else
static inline void arcade_trim(void) {}
static inline void arcade_page_out_hidden(void) {}
#endif

#endif
