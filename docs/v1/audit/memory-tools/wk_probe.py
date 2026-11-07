import gi, sys
gi.require_version('Gtk', '3.0'); gi.require_version('WebKit2', '4.1')
from gi.repository import Gtk, WebKit2, GLib
html = sys.argv[1] if len(sys.argv) > 1 else '<h1>hello</h1>'
w = Gtk.Window(); w.set_default_size(940, 780)
v = WebKit2.WebView(); w.add(v); v.load_html(html, 'http://localhost/'); w.show_all()
Gtk.main()
