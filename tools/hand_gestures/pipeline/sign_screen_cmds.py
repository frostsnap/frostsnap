"""usage: python3 sign_screen_cmds.py <frames dir> > sign.cmds

A widget_simulator script for `sign_prompt`: three swipes up, then a hold, with a screenshot
every 40 ms. Frame 40 is the first page; frame 235 is the hold-to-confirm page before the press.
"""
import sys
d=sys.argv[1]; n=0
def frames(ms):
    global n
    for _ in range(ms//40):
        print(f"screenshot {d}/f{n:04d}.png"); print("wait 40"); n+=1
def swipe():
    print("touch 120,240"); y=240
    while y>80:
        print("wait 16"); print(f"drag 120,{y} 120,{y-20}"); y-=20
    print("wait 16"); print("release 120,80")
frames(2400)
for i in range(3):
    swipe(); frames(2400)
print("touch 120,215"); frames(4000)
print("release 120,215"); frames(2400)
print("quit")
