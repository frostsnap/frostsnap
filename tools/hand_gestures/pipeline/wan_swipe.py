"""Submit a key image to fal.ai Wan 3.0 image-to-video and download the result.

usage: FAL_ENV_FILE=<.env path> python3 -I wan_swipe.py <key.png> <out.mp4> "<hand description>"
The key is read from FAL_AI_KEY in that file and never printed.
"""
import base64, json, os, sys, time, urllib.request

ENDPOINT = "alibaba/wan-3.0/image-to-video"

key_png, out_mp4, hand = sys.argv[1], sys.argv[2], sys.argv[3]
fal_key = None
with open(os.environ["FAL_ENV_FILE"]) as f:
    for line in f:
        name, _, value = line.strip().removeprefix("export ").partition("=")
        if name.strip() == "FAL_AI_KEY":
            fal_key = value.strip().strip("'\"")
if not fal_key:
    sys.exit("FAL_AI_KEY not found in the env file")

prompt = (
    "Static overhead shot. The camera is locked: no camera movement, no zoom, no pan. "
    "Only the hand moves; the phone, the small device and everything on its screen stay "
    "exactly as they are, with no glow, light effects or changes on the screen. "
    f"{hand} performs exactly two upward swipes on the small device's screen, in this order: "
    "first swipe: the hand moves up until the fingertip touches the bottom of the device's screen, "
    "then, keeping the fingertip pressed on the screen, the finger slides straight upward to the "
    "top of the screen in one smooth stroke, and the fingertip lifts off at the top. "
    "The hand moves back down, above the screen without touching it, and pauses briefly. "
    "Second swipe: the fingertip touches the bottom of the screen again and slides straight "
    "upward to the top in one smooth stroke, then lifts off. "
    "Finally the hand returns to exactly its starting position below the device and stays "
    "still there. The finger never slides downward while touching the screen. "
    "The hand does not leave the frame."
)

with open(key_png, "rb") as f:
    data_uri = "data:image/png;base64," + base64.b64encode(f.read()).decode()
end_png = os.environ.get("END_IMAGE")
end_uri = None
if end_png:
    with open(end_png, "rb") as f:
        end_uri = "data:image/png;base64," + base64.b64encode(f.read()).decode()

body = {
    "start_image_url": data_uri,
    "prompt": prompt,
    "resolution": "720p",
    "aspect_ratio": "adaptive",
    "duration": int(os.environ.get("DURATION", "5")),
    "audio": False,
    "enable_prompt_expansion": False,
}
if end_uri:
    body["end_image_url"] = end_uri
headers = {"Authorization": f"Key {fal_key}", "Content-Type": "application/json"}


def call(url, payload=None):
    req = urllib.request.Request(
        url,
        data=None if payload is None else json.dumps(payload).encode(),
        headers=headers,
        method="GET" if payload is None else "POST",
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


sub = call(f"https://queue.fal.run/{ENDPOINT}", body)
status_url, response_url = sub["status_url"], sub["response_url"]
print("submitted", sub["request_id"], flush=True)
while True:
    st = call(status_url)
    if st["status"] == "COMPLETED":
        break
    if st["status"] not in ("IN_QUEUE", "IN_PROGRESS"):
        sys.exit(f"failed: {st}")
    time.sleep(8)
res = call(response_url)
urllib.request.urlretrieve(res["video"]["url"], out_mp4)
print("saved", out_mp4, res.get("duration"), "s, seed", res.get("seed"))
