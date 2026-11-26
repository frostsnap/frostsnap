import bpy, bmesh, math, json, sys, os
from mathutils import Vector, Matrix
from bpy_extras.object_utils import world_to_camera_view

argv = sys.argv[sys.argv.index("--")+1:] if "--" in sys.argv else []
COLOUR = argv[0] if argv else "blue"
OUT = argv[1] if len(argv)>1 else "/tmp/out"
SAMPLES = int(argv[2]) if len(argv)>2 else 64
LAYER = argv[3] if len(argv)>3 else "all"
COLOUR2 = argv[4] if len(argv)>4 else "orange"
RES = (1500, 1000)  # 180mm wide ortho: 8.33 px/mm
HERE = os.path.dirname(os.path.abspath(__file__))
STL = os.environ.get("FROSTSNAP_CASE_STL", os.path.expanduser("~/src/frostsnap-hardware/SFv2.7.3_3dp.stl"))

CASES = {
    "blue":   ((0.03, 0.22, 0.42), 0.32),
    "orange": ((0.62, 0.24, 0.04), 0.32),
    "silver": ((0.62, 0.64, 0.66), 0.25),
    "black":  ((0.025, 0.025, 0.03), 0.35),
    "red":    ((0.45, 0.03, 0.03), 0.32),
    "plain":  ((0.16, 0.16, 0.17), 0.6),
}

bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.unit_settings.scale_length = 0.001

def mat(name, color, metal=0.0, rough=0.4, emit=None, coat=0.0, alpha=1.0, trans=0.0):
    m = bpy.data.materials.new(name); m.use_nodes = True
    b = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    b.inputs["Base Color"].default_value = (*color, 1)
    b.inputs["Metallic"].default_value = metal
    b.inputs["Roughness"].default_value = rough
    if coat: b.inputs["Coat Weight"].default_value = coat
    if trans: b.inputs["Transmission Weight"].default_value = trans
    if emit:
        b.inputs["Emission Color"].default_value = (*emit[0], 1)
        b.inputs["Emission Strength"].default_value = emit[1]
    return m

def rounded_box(name, sx, sy, sz, r, segs=10, bevel_z=0.0):
    bpy.ops.mesh.primitive_cube_add(size=1)
    o = bpy.context.object; o.name = name
    o.scale = (sx, sy, sz); bpy.ops.object.transform_apply(scale=True)
    me = o.data; bm = bmesh.new(); bm.from_mesh(me)
    edges = [e for e in bm.edges if abs((e.verts[0].co - e.verts[1].co).z) > 1e-6]
    bmesh.ops.bevel(bm, geom=edges, offset=r, segments=segs, affect='EDGES', profile=0.5)
    bm.to_mesh(me); bm.free()
    if bevel_z:
        mod = o.modifiers.new("b", "BEVEL"); mod.width = bevel_z; mod.segments = 4; mod.limit_method = 'ANGLE'
    for p in me.polygons: p.use_smooth = True
    return o

# Device local frame: screen up +Z, USB toward +Y.
bpy.ops.wm.stl_import(filepath=STL)
case = bpy.context.selected_objects[0]; case.name = "case"
for p in case.data.polygons: p.use_smooth = True
case.data.materials.append(mat("case", CASES[COLOUR][0], metal=0.85, rough=CASES[COLOUR][1]))
mod = case.modifiers.new("ws", "WEIGHTED_NORMAL")

import sys as _s; _s.path.insert(0, HERE)
from device_front import build_front
glass, inner, disp, DW, DH = build_front(bpy, bmesh, Vector, mat, rounded_box, os.path.join(HERE, "outline.svg"))
plug = rounded_box("plug", 8.3, 9.0, 2.5, 1.2, segs=6); plug.location = (0, 21.0 + 4.0, 4.1)
plug.data.materials.append(mat("plug", (0.75, 0.76, 0.78), metal=1.0, rough=0.22))
boot = rounded_box("boot", 11.0, 2.4, 5.0, 2.0, segs=6); boot.location = (0, 21.6, 4.1)
boot.data.materials.append(mat("boot", (0.03, 0.03, 0.035), rough=0.45))

dev = bpy.data.objects.new("device", None); sc.collection.objects.link(dev)
for o in (case, inner, glass, disp, plug, boot): o.parent = dev
for o in (case, inner, glass, disp, plug, boot): o["dev"] = 1
dev2 = bpy.data.objects.new("device2", None); sc.collection.objects.link(dev2)
dev2.parent = dev; dev2.location = (0, -43.0, 0)
for o in (case, inner, glass, disp, plug, boot):
    c = o.copy(); c.data = o.data.copy(); sc.collection.objects.link(c)
    c.parent = dev2; c["dev"] = 2; c.hide_render = o.hide_render
    if o is case:
        c.data.materials.clear(); c.data.materials.append(mat("case2", CASES[COLOUR2][0], metal=0.85, rough=CASES[COLOUR2][1]))
    if o is disp: disp2 = c

L_W, L_D, L_H = 304.0, 214.0, 11.5
base = rounded_box("base", L_W, L_D, L_H, 9.0, segs=12, bevel_z=2.2); base.location = (0, 0, L_H/2)
alu = mat("alu", (0.55, 0.56, 0.58), metal=1.0, rough=0.32)
base.data.materials.append(alu)
deck = rounded_box("deck", 270, 118, 0.6, 3, segs=6); deck.location = (0, 35, L_H + 0.05)
deck.data.materials.append(mat("keys", (0.015, 0.015, 0.017), rough=0.6))
keymat = mat("keycap", (0.03, 0.03, 0.033), rough=0.55)
proto = rounded_box("key", 15.6, 15.6, 0.8, 2.2, segs=4); proto.location = (9999, 0, 0)
proto.data.materials.append(keymat)
for _p in proto.data.polygons: _p.use_smooth = False
pitch = 18.6
for r in range(6):
    h = 7.0 if r == 0 else 15.6
    y = 35 + 118/2 - 6 - (0 if r == 0 else 11 + (r - 1) * pitch + 4)
    x = -270/2 + 6 + 15.6/2
    while x < 270/2 - 6:
        k = proto.copy(); sc.collection.objects.link(k)
        k.scale = (1, h / 15.6, 1)
        k.location = (x, y, L_H + 0.75)
        x += pitch
pad = rounded_box("pad", 130, 78, 0.4, 4, segs=6); pad.location = (0, -60, L_H + 0.02)
pad.data.materials.append(mat("pad", (0.5, 0.51, 0.53), metal=1.0, rough=0.42))
port = rounded_box("port", 3, 9.2, 3.4, 1.4, segs=6); port.location = (-L_W/2 + 0.6, 40, 5.6)
port.data.materials.append(mat("port", (0.0, 0.0, 0.0), rough=0.9))

lid = bpy.data.objects.new("lid", None); sc.collection.objects.link(lid)
lid.location = (0, L_D/2 - 4, L_H)
shell = rounded_box("shell", L_W, 5.0, 206, 9.0, segs=12, bevel_z=1.5)
shell.rotation_euler = (0, 0, 0)
bpy.ops.object.select_all(action='DESELECT')
shell.data.materials.append(alu); shell.parent = lid; shell.location = (0, 0, 103)
bez = rounded_box("bezel", L_W - 8, 0.8, 198, 7.0, segs=10); bez.parent = lid; bez.location = (0, -2.6, 103)
bez.data.materials.append(mat("bezel", (0.01, 0.01, 0.012), rough=0.08, coat=1.0))
bpy.ops.mesh.primitive_plane_add(size=1); scr = bpy.context.object; scr.name = "lcd"
scr.rotation_euler = (math.radians(90), 0, 0); scr.scale = (276, 176, 1)
scr.parent = lid; scr.location = (0, -3.05, 105)
scr.data.materials.append(mat("lcd", (0, 0, 0), rough=0.06, coat=1.0, emit=((0.32,0.42,0.55), 1.2)))
lid.rotation_euler = (math.radians(-18), 0, 0)  # opened ~108°
lid.hide_render = True
for ch in lid.children: ch.hide_render = True

# device into the left port, lying flat, screen up, USB end pointing +X into the laptop
dev.rotation_euler = (0, 0, math.radians(-90))
dev.location = (-L_W/2 - 21.0 - 9.0 + 6.5, 40, 5.6 - 4.1)

bpy.ops.mesh.primitive_plane_add(size=3000); floor = bpy.context.object
floor.is_shadow_catcher = True

def area(name, loc, size, power, color=(1,1,1)):
    l = bpy.data.lights.new(name, "AREA"); l.size = size; l.energy = power; l.color = color
    o = bpy.data.objects.new(name, l); sc.collection.objects.link(o); o.location = loc
    o.rotation_euler = (Vector((0,0,0)) - Vector(loc)).to_track_quat('-Z','Y').to_euler()
    return o
area("key", (-420, -300, 520), 420, 2.0e6, (1.0, 0.97, 0.93))
area("rim", (300, 500, 300), 400, 9e5, (0.75, 0.85, 1.0))
area("fill", (-700, 100, 150), 500, 5e5)
w = bpy.data.worlds.new("w"); w.use_nodes = True
bg = next(n for n in w.node_tree.nodes if n.type == "BACKGROUND"); bg.inputs[0].default_value = (0.06, 0.065, 0.075, 1); bg.inputs[1].default_value = 1.0
sc.world = w

cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam")); sc.collection.objects.link(cam); sc.camera = cam
cam.data.type = 'ORTHO'; cam.data.ortho_scale = 180; cam.data.clip_start = 1; cam.data.clip_end = 10000
cam.location = (-L_W/2 - 20 + 55, 40, 400); cam.rotation_euler = (0, 0, 0)
sc.render.engine = 'CYCLES'; sc.cycles.samples = SAMPLES; sc.cycles.use_denoising = True
try: sc.cycles.device = 'GPU'; bpy.context.preferences.addons['cycles'].preferences.compute_device_type = 'METAL'; bpy.context.preferences.addons['cycles'].preferences.get_devices()
except Exception as e: print("gpu", e)
for d in bpy.context.preferences.addons['cycles'].preferences.devices: d.use = True
sc.render.film_transparent = True
sc.render.resolution_x, sc.render.resolution_y = RES
sc.view_settings.view_transform = 'AgX'; sc.view_settings.look = 'AgX - Medium High Contrast'
sc.render.image_settings.file_format = 'PNG'; sc.render.image_settings.color_mode = 'RGBA'
sc.render.filepath = OUT + ".png"

DEV_PARTS = {"case","lcd_panel","glass","display","plug","boot"}
for o in sc.objects:
    if o.type not in ("MESH",): continue
    d = o.get("dev", 0)
    if LAYER == "base" and d: o.hide_render = True
    if LAYER == "device" and d != 1: o.is_holdout = True
    if LAYER == "device" and d == 2: o.hide_render = True
    if LAYER == "device2" and d != 2: o.is_holdout = True
    if LAYER == "device" and d == 0 and False: pass
bpy.context.view_layer.update()
def proj(v):
    co = world_to_camera_view(sc, cam, v); return [co.x * RES[0], (1 - co.y) * RES[1]]
plug_axis = (dev.matrix_world.to_3x3() @ Vector((0, 1, 0))).normalized()
centre = dev.matrix_world @ Vector((0, 0, 4))
def quad(o, w, h, axes):
    pts = []
    for sx, sy in ((-1, 1), (1, 1), (1, -1), (-1, -1)):  # TL TR BR BL as seen
        local = Vector((sx * 0.5, sy * 0.5, 0))
        co = world_to_camera_view(sc, cam, o.matrix_world @ local)
        pts.append([co.x * RES[0], (1 - co.y) * RES[1]])
    return pts
json.dump({"res": RES, "device": quad(disp, DW, DH, None), "device2": quad(disp2, DW, DH, None), "host": quad(scr, 0, 0, None), "dev_centre": proj(centre), "dev_out": proj(centre - plug_axis * 10)}, open(OUT + ".json", "w"), indent=1)
bpy.ops.render.render(write_still=True)
