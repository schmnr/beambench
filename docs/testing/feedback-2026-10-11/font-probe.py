import gi,sys
from pathlib import Path
gi.require_version('Gtk','3.0')
gi.require_version('WebKit2','4.1')
from gi.repository import Gtk,WebKit2,GLib
print('WebKit',WebKit2.get_major_version(),WebKit2.get_minor_version(),WebKit2.get_micro_version(),flush=True)
window=Gtk.Window();window.set_default_size(600,300)
view=WebKit2.WebView();window.add(view)
def crashed(view,reason):
 print('WEB PROCESS TERMINATED',reason,flush=True)
 Gtk.main_quit()
view.connect('web-process-terminated',crashed)
view.connect('load-changed',lambda view,event: print('load',event,flush=True))
font='file:///evidence/Noto-COLRv1.ttf'
html=f'<html><style>@font-face{{font-family:Probe;src:url({font})}} body{{font-family:Probe;font-size:48px}}</style><body>&#x1F680; &#x1F600; &#x1F308; &#x1F1FA;&#x1F1F8;</body></html>'
view.load_html(html,'file:///evidence/')
window.show_all()
def finished():
 print('PROBE SURVIVED',flush=True);Gtk.main_quit();return False
GLib.timeout_add_seconds(10,finished)
Gtk.main()
