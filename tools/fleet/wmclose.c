/* Send WM_DELETE_WINDOW to a window, as a title bar's close button does (Xvfb has no window manager).
   XSync before exit: an X server that polls a client after it has gone sees the request and the hang-up together and
   drops the client without reading it, so a close sent with only XFlush is lost whenever the machine is busy. */
#include <X11/Xlib.h>
#include <stdlib.h>
int main(int c, char **v) {
  Display *d = XOpenDisplay(0); if (!d || c < 2) return 1;
  Window w = strtoul(v[1], 0, 0);
  Atom p = XInternAtom(d, "WM_PROTOCOLS", 0), q = XInternAtom(d, "WM_DELETE_WINDOW", 0);
  XEvent e = {0}; e.xclient.type = ClientMessage; e.xclient.window = w; e.xclient.message_type = p; e.xclient.format = 32;
  e.xclient.data.l[0] = q; e.xclient.data.l[1] = CurrentTime;
  Status sent = XSendEvent(d, w, 0, NoEventMask, &e);
  XSync(d, False); XCloseDisplay(d);
  return sent ? 0 : 2;
}
