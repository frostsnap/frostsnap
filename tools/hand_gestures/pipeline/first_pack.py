"""usage: python first_pack.py <dir holding hand-swipe/ and hand-hold/> <out dir>

Packs extract_hand.py's frames into 248×405 cells, 10 to a row: the swipe's first 95 frames (one
swipe, rest pose to rest pose) and all 210 of the hold's. The box is the union of the hand's
extent over those frames.
"""
import sys

from PIL import Image

K, A = sys.argv[1], sys.argv[2]
box=(317,319,814,1130); fw,fh=248,405; cols=10
for clip,n,name in (('hand-swipe',95,'hand-swipe-sheet'),('hand-hold',210,'hand-hold-sheet')):
    rows=(n+cols-1)//cols
    sheet=Image.new('RGBA',(cols*fw,rows*fh),(0,0,0,0))
    for i in range(n):
        f=Image.open(f'{K}/{clip}/{i:04d}.png').crop(box).resize((fw,fh),Image.LANCZOS)
        sheet.paste(f,((i%cols)*fw,(i//cols)*fh))
    sheet.save(f'{A}/{name}.webp','WEBP',quality=82,alpha_quality=90,method=6)
    print(name, sheet.size)
