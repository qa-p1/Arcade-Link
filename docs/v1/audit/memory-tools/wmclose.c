#include <X11/Xlib.h>
#include <stdlib.h>
int main(int c, char **v) {
  Display *d = XOpenDisplay(0); if (!d || c < 2) return 1;
  Window w = strtoul(v[1], 0, 0);
  Atom p = XInternAtom(d, "WM_PROTOCOLS", 0), q = XInternAtom(d, "WM_DELETE_WINDOW", 0);
  XEvent e = {0}; e.xclient.type = ClientMessage; e.xclient.window = w; e.xclient.message_type = p; e.xclient.format = 32;
  e.xclient.data.l[0] = q; e.xclient.data.l[1] = CurrentTime;
  XSendEvent(d, w, 0, NoEventMask, &e); XFlush(d); return 0;
}
