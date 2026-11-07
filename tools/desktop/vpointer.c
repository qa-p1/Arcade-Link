/* Virtual pointer for the desktop harness: reads commands on stdin and sends
 * them through zwlr_virtual_pointer_v1, so the compositor sees real motion,
 * enter/leave and button events.
 *
 *   move X Y         absolute position in the layout (logical pixels)
 *   press | release  left button
 *   click [right]    press and release
 *
 * Build: wayland-scanner client-header/private-code on
 * wlr-virtual-pointer-unstable-v1.xml, then cc vpointer.c protocol.c -lwayland-client */
#include <linux/input-event-codes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wayland-client.h>
#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"

static struct wl_seat *seat;
static struct zwlr_virtual_pointer_manager_v1 *manager;

static void global(void *data, struct wl_registry *r, uint32_t name, const char *iface, uint32_t version)
{
    (void)data;
    if (!strcmp(iface, wl_seat_interface.name))
        seat = wl_registry_bind(r, name, &wl_seat_interface, 1);
    else if (!strcmp(iface, zwlr_virtual_pointer_manager_v1_interface.name))
        manager = wl_registry_bind(r, name, &zwlr_virtual_pointer_manager_v1_interface, version < 2 ? version : 2);
}
static void global_remove(void *data, struct wl_registry *r, uint32_t name) { (void)data; (void)r; (void)name; }
static const struct wl_registry_listener listener = {global, global_remove};

static uint32_t now_ms(void)
{
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint32_t)(t.tv_sec * 1000 + t.tv_nsec / 1000000);
}

int main(int argc, char **argv)
{
    unsigned width = argc > 2 ? (unsigned)atoi(argv[1]) : 3840, height = argc > 2 ? (unsigned)atoi(argv[2]) : 1080;
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "vpointer: no Wayland display\n");
        return 1;
    }
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &listener, NULL);
    wl_display_roundtrip(display);
    if (!manager || !seat) {
        fprintf(stderr, "vpointer: compositor lacks zwlr_virtual_pointer_manager_v1\n");
        return 1;
    }
    struct zwlr_virtual_pointer_v1 *p = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, seat);
    char line[128];
    setvbuf(stdout, NULL, _IOLBF, 0);
    while (fgets(line, sizeof line, stdin)) {
        int x, y;
        if (sscanf(line, "move %d %d", &x, &y) == 2) {
            zwlr_virtual_pointer_v1_motion_absolute(p, now_ms(), (uint32_t)x, (uint32_t)y, width, height);
        } else if (!strncmp(line, "press", 5) || !strncmp(line, "release", 7) || !strncmp(line, "click", 5)) {
            uint32_t button = strstr(line, "right") ? BTN_RIGHT : BTN_LEFT;
            if (line[0] != 'r')
                zwlr_virtual_pointer_v1_button(p, now_ms(), button, WL_POINTER_BUTTON_STATE_PRESSED);
            if (line[0] == 'c') {
                zwlr_virtual_pointer_v1_frame(p);
                wl_display_roundtrip(display);
            }
            if (line[0] != 'p')
                zwlr_virtual_pointer_v1_button(p, now_ms(), button, WL_POINTER_BUTTON_STATE_RELEASED);
        } else {
            fprintf(stderr, "vpointer: unknown command %s", line);
            continue;
        }
        zwlr_virtual_pointer_v1_frame(p);
        wl_display_roundtrip(display);
        puts("ok");
    }
    zwlr_virtual_pointer_v1_destroy(p);
    wl_display_roundtrip(display);
    return 0;
}
