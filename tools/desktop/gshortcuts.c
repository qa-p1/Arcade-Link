/* Hyprland global-shortcuts client for the portal stand-in. It does what
 * xdg-desktop-portal-hyprland does for a bound shortcut: registers it through
 * hyprland_global_shortcuts_manager_v1, so `hyprctl globalshortcuts` lists it
 * as "<app_id>:<id>" and `hl.dsp.global("<app_id>:<id>")` reaches it.
 *
 * stdin, one command per line, fields separated by tabs:
 *   register\t<id>\t<app_id>\t<description>\t<trigger_description>
 * stdout, one line per command and per compositor event:
 *   ok | pressed <app_id>:<id> | released <app_id>:<id>
 *
 * Build: wayland-scanner client-header/private-code on
 * hyprland-global-shortcuts-v1.xml, then cc gshortcuts.c protocol.c -lwayland-client */
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wayland-client.h>
#include "hyprland-global-shortcuts-v1-client-protocol.h"

static struct hyprland_global_shortcuts_manager_v1 *manager;

static void global(void *data, struct wl_registry *r, uint32_t name, const char *iface, uint32_t version)
{
    (void)data;
    (void)version;
    if (!strcmp(iface, hyprland_global_shortcuts_manager_v1_interface.name))
        manager = wl_registry_bind(r, name, &hyprland_global_shortcuts_manager_v1_interface, 1);
}
static void global_remove(void *data, struct wl_registry *r, uint32_t name) { (void)data; (void)r; (void)name; }
static const struct wl_registry_listener registry_listener = {global, global_remove};

static void pressed(void *data, struct hyprland_global_shortcut_v1 *s, uint32_t hi, uint32_t lo, uint32_t ns)
{
    (void)s; (void)hi; (void)lo; (void)ns;
    printf("pressed %s\n", (const char *)data);
}
static void released(void *data, struct hyprland_global_shortcut_v1 *s, uint32_t hi, uint32_t lo, uint32_t ns)
{
    (void)s; (void)hi; (void)lo; (void)ns;
    printf("released %s\n", (const char *)data);
}
static const struct hyprland_global_shortcut_v1_listener shortcut_listener = {pressed, released};

/* Splits LINE at tabs into at most N fields; returns how many it found. */
static int fields(char *line, char **out, int n)
{
    int count = 0;
    line[strcspn(line, "\n")] = 0;
    for (char *p = line; count < n; count++) {
        out[count] = p;
        char *tab = strchr(p, '\t');
        if (!tab)
            return count + 1;
        *tab = 0;
        p = tab + 1;
    }
    return count;
}

int main(void)
{
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "gshortcuts: no Wayland display\n");
        return 1;
    }
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!manager) {
        fprintf(stderr, "gshortcuts: compositor lacks hyprland_global_shortcuts_manager_v1\n");
        return 1;
    }
    setvbuf(stdout, NULL, _IOLBF, 0);
    char line[1024];
    for (;;) {
        while (wl_display_prepare_read(display) != 0)
            wl_display_dispatch_pending(display);
        if (wl_display_flush(display) < 0) {
            wl_display_cancel_read(display);
            break;
        }
        struct pollfd fds[2] = {{wl_display_get_fd(display), POLLIN, 0}, {STDIN_FILENO, POLLIN, 0}};
        if (poll(fds, 2, -1) < 0) {
            wl_display_cancel_read(display);
            break;
        }
        if (fds[0].revents & POLLIN) {
            if (wl_display_read_events(display) < 0)
                break;
        } else {
            wl_display_cancel_read(display);
        }
        if (wl_display_dispatch_pending(display) < 0)
            break;
        if (fds[0].revents & (POLLERR | POLLHUP))
            break;
        if (fds[1].revents & (POLLIN | POLLHUP)) {
            if (!fgets(line, sizeof line, stdin))
                break;
            char *f[5];
            if (fields(line, f, 5) == 5 && !strcmp(f[0], "register")) {
                size_t len = strlen(f[1]) + strlen(f[2]) + 2;
                char *target = malloc(len);
                snprintf(target, len, "%s:%s", f[2], f[1]);
                struct hyprland_global_shortcut_v1 *s =
                    hyprland_global_shortcuts_manager_v1_register_shortcut(manager, f[1], f[2], f[3], f[4]);
                hyprland_global_shortcut_v1_add_listener(s, &shortcut_listener, target);
                if (wl_display_roundtrip(display) < 0)
                    break;
                puts("ok");
            } else {
                fprintf(stderr, "gshortcuts: unknown command %s\n", line);
            }
        }
    }
    int err = wl_display_get_error(display);
    if (err)
        fprintf(stderr, "gshortcuts: Wayland connection failed (error %d)\n", err);
    return err ? 1 : 0;
}
