def build_front(bpy, bmesh, Vector, mat, rounded_box, svg_path):
    bpy.ops.import_curve.svg(filepath=svg_path)
    cu = [o for o in bpy.context.scene.objects if o.type == 'CURVE'][-1]
    bpy.context.view_layer.objects.active = cu
    for o in bpy.context.selected_objects: o.select_set(False)
    cu.select_set(True)
    bpy.ops.object.origin_set(type='ORIGIN_GEOMETRY', center='BOUNDS')
    cu.location = (0, 0, 0)
    bpy.context.view_layer.update()
    cu.data.dimensions = '2D'; cu.data.fill_mode = 'BOTH'
    bpy.ops.object.convert(target='MESH')
    glass = bpy.context.object; glass.name = "glass"
    s = 33.3 / glass.dimensions.x
    glass.scale = (s, s, 1)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    bm = bmesh.new(); bm.from_mesh(glass.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.01)
    r = bmesh.ops.extrude_face_region(bm, geom=bm.faces[:])
    for v in [g for g in r["geom"] if isinstance(g, bmesh.types.BMVert)]: v.co.z += 0.7
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    bm.to_mesh(glass.data); bm.free()
    bv = glass.modifiers.new("b", "BEVEL"); bv.width = 0.45; bv.segments = 6; bv.limit_method = 'ANGLE'
    for p in glass.data.polygons: p.use_smooth = True
    glass.location = (0, 0, 8.2 - 0.45)
    glass.data.materials.clear()
    glass.data.materials.append(mat("glass", (0.022, 0.03, 0.036), rough=0.03, coat=1.0))
    # Display window from the CAD front outline (30×35 mm, r4), set away from the USB end.
    DW, DH, DY = 30.0, 35.0, -1.55
    lcd = rounded_box("lcd_panel", DW, DH, 0.05, 4.0, segs=10); lcd.location = (0, DY, 8.2 + 0.27)
    lcd.data.materials.append(mat("lcdpanel", (0.006, 0.007, 0.008), rough=0.35))
    bpy.ops.mesh.primitive_plane_add(size=1); disp = bpy.context.object; disp.name = "display"
    disp.scale = (DW, DH, 1); disp.location = (0, DY, 8.2 + 0.31)
    disp.hide_render = True
    return glass, lcd, disp, DW, DH
