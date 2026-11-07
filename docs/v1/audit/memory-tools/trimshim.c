// LD_PRELOAD experiment shim: polls $TRIMDIR/go (mtime) and runs a memory-release action in-process. Measurement aid only.
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <pthread.h>
#include <malloc.h>
#include <sys/mman.h>
#include <sys/stat.h>
#ifndef MADV_PAGEOUT
#define MADV_PAGEOUT 21
#endif
#ifndef MADV_COLD
#define MADV_COLD 20
#endif
static void act(const char *mode) {
  if (strstr(mode, "trim")) malloc_trim(0);
  int dn = strstr(mode, "dontneed") != NULL, po = strstr(mode, "pageout") != NULL, pa = strstr(mode, "anon") != NULL;
  if (!dn && !po && !pa) return;
  FILE *f = fopen("/proc/self/maps", "r"); if (!f) return;
  static char line[4096];
  static unsigned long st[20000], en[20000]; static char kind[20000]; int n = 0;
  while (fgets(line, sizeof line, f) && n < 20000) {
    unsigned long a, b; char perm[8]; char path[3000] = "";
    unsigned long off; char dev[16]; unsigned long ino;
    int k = sscanf(line, "%lx-%lx %7s %lx %15s %lu %2999[^\n]", &a, &b, perm, &off, dev, &ino, path);
    if (k < 6) continue;
    char *p = path; while (*p == ' ') p++;
    int file = p[0] == '/' && !strstr(p, "(deleted)");
    int w = strchr(perm, 'w') != NULL, x = strchr(perm, 'x') != NULL;
    if (file && !w) { st[n] = a; en[n] = b; kind[n] = x ? 'x' : 'r'; n++; }          // read-only file mappings
    else if (pa && !file && p[0] != '[' && w) { st[n] = a; en[n] = b; kind[n] = 'a'; n++; } // anon private rw
  }
  fclose(f);
  for (int i = 0; i < n; i++) {
    size_t len = en[i] - st[i];
    if (kind[i] == 'a') { if (pa) madvise((void *)st[i], len, MADV_PAGEOUT); continue; }
    if (po) madvise((void *)st[i], len, MADV_PAGEOUT);
    if (dn && kind[i] == 'x') madvise((void *)st[i], len, MADV_DONTNEED); // text only: never COW'd, refaults from page cache
  }
}
static void *run(void *arg) {
  const char *dir = getenv("TRIMDIR"); if (!dir) return 0;
  char path[512]; snprintf(path, sizeof path, "%s/go", dir);
  time_t last = 0; struct stat s;
  if (stat(path, &s) == 0) last = s.st_mtime;
  long lastn = 0; if (stat(path, &s) == 0) lastn = s.st_mtim.tv_nsec;
  for (;;) {
    usleep(100000);
    if (stat(path, &s) != 0) continue;
    if (s.st_mtime != last || s.st_mtim.tv_nsec != lastn) {
      last = s.st_mtime; lastn = s.st_mtim.tv_nsec;
      char mode[128] = ""; FILE *f = fopen(path, "r"); if (f) { if (!fgets(mode, sizeof mode, f)) mode[0] = 0; fclose(f); }
      act(mode);
    }
  }
  return 0;
}
__attribute__((constructor)) static void init(void) {
  if (!getenv("TRIMDIR")) return;
  pthread_t t; pthread_attr_t a; pthread_attr_init(&a); pthread_attr_setstacksize(&a, 256 * 1024);
  pthread_create(&t, &a, run, 0); pthread_detach(t);
}
