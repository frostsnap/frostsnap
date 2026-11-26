"""Writes outline.svg, the device's front outline that device_front.py imports, from the path the
app's device drawing already carries."""
import os, re

HERE = os.path.dirname(os.path.abspath(__file__))
src=open(os.path.join(HERE, '../../frostsnapp/lib/device_action_fullscreen_dialog.dart')).read()
d=re.search(r'id="Binder022_f0000"\s+d="([^"]+)"',src).group(1)
outer=d.split('Z')[0]+'Z'
open(os.path.join(HERE, 'outline.svg'),'w').write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="-20 -25 40 50" width="40mm" height="50mm"><path d="{outer}" fill="#000"/></svg>')
print(outer[:80])
