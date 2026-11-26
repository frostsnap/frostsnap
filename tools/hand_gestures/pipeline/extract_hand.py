"""Cut the frosted hand out of a Wan clip as an RGBA frame sequence.

usage: python extract_hand.py <clip.mp4> <hand-free background.png> <out dir>

SAM 2.1 finds the hand; each pixel inside its mask is un-blended against the hand-free
background, so what was behind the hand is not baked in.
"""
import os, subprocess, sys, tempfile

import numpy as np
import torch
from PIL import Image
from transformers import Sam2VideoModel, Sam2VideoProcessor

clip, bg_path, out_dir = sys.argv[1:4]
os.makedirs(out_dir, exist_ok=True)

with tempfile.TemporaryDirectory() as tmp:
    subprocess.run(["ffmpeg", "-loglevel", "error", "-i", clip, f"{tmp}/%04d.png"], check=True)
    names = sorted(os.listdir(tmp))
    frames = [Image.open(f"{tmp}/{n}").convert("RGB") for n in names]
W, H = frames[0].size
bg = np.asarray(Image.open(bg_path).convert("RGB").resize((W, H), Image.LANCZOS), dtype=np.float32)

device = "mps" if torch.backends.mps.is_available() else "cpu"
model = Sam2VideoModel.from_pretrained("facebook/sam2.1-hiera-large").to(device, dtype=torch.float32)
processor = Sam2VideoProcessor.from_pretrained("facebook/sam2.1-hiera-large")
session = processor.init_video_session(video=frames, inference_device=device, dtype=torch.float32)

# Key-frame pose in the 814×1130 clip: fingertip, knuckles, back of hand; and the device as a
# negative so the mask never spreads onto it.
pos = [(393, 712), (430, 800), (480, 900), (540, 1020)]
neg = [(395, 450), (395, 150)]
points = [[[[x * W / 814, y * H / 1130] for x, y in pos + neg]]]
labels = [[[1] * len(pos) + [0] * len(neg)]]
processor.add_inputs_to_inference_session(
    inference_session=session, frame_idx=0, obj_ids=1, input_points=points, input_labels=labels
)
model(inference_session=session, frame_idx=0)

masks = {}
for out in model.propagate_in_video_iterator(session):
    m = processor.post_process_masks(
        [out.pred_masks], original_sizes=[[session.video_height, session.video_width]], binarize=False
    )[0]
    masks[out.frame_idx] = torch.sigmoid(m[0, 0]).float().cpu().numpy() if m.min() < 0 else m[0, 0].float().cpu().numpy()

# The hand was composited at a fixed opacity, so un-blending each pixel against the hand-free
# background recovers its own shaded colour (fingers, knuckles, palm) without what was behind it.
OPACITY = 0.6
print("frames", len(frames))
for i, fr in enumerate(frames):
    f = np.asarray(fr, dtype=np.float32)
    soft = np.clip((masks.get(i, np.zeros((H, W))) - 0.3) / 0.4, 0, 1)
    colour = np.clip((f - bg * (1 - OPACITY)) / OPACITY, 0, 255)
    alpha = OPACITY * soft
    rgba = np.dstack([colour, alpha[..., None] * 255]).astype(np.uint8)
    Image.fromarray(rgba, "RGBA").save(f"{out_dir}/{i:04d}.png")
print("done", out_dir)
