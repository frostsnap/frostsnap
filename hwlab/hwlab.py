#!/usr/bin/env python3
"""Camera-read hardware experiments on an allowlisted dev device.

  hwlab camera start|stop     phone camera -> v4l2 loopback via scrcpy
  hwlab snap [NAME]           one frame, rotated, plus the decoded QR
  hwlab calibrate [N]         decode rate over N snaps of the card
  hwlab run SPEC [VARIANT...] build, flash, watch, record
"""

import argparse
import datetime
import json
import os
import re
import secrets
import shutil
import signal
import subprocess
import sys
import time
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
HOME = Path(os.environ.get("HWLAB_HOME", Path.home() / "hwlab"))
CONFIG = HOME / "config.toml"
BUILD = HOME / "build"
RUNS = HOME / "runs"
RISCV_BIN = Path.home() / "repos/frostsnap/riscv/bin"

# Byte-mode capacity at ECC medium for versions 1..8, matching device/src/hwlab/mod.rs.
QR_CAPACITY_M = [14, 26, 42, 62, 84, 106, 122, 152]
# The card layout in device pixels, also from device/src/hwlab/mod.rs.
SCREEN = (240, 280)
QR_AREA = (0, 62, 240, 218)
QUIET_ZONE = 2

# Experiment patches must never program eFuses or touch provisioning.
FORBIDDEN_IN_PATCH = re.compile(
    r"pgm_cmd|pgm_data|pgm_check|send_write_command|write_efuses|write_block|write_key_purposes"
    r"|set_efuse_key|run_factory_provisioning|EFUSE::regs|erase_flash|software_reset_cpu_and_burn",
    re.IGNORECASE,
)


def die(msg):
    print(f"hwlab: {msg}", file=sys.stderr)
    sys.exit(1)


def config():
    if not CONFIG.exists():
        return {}
    return tomllib.loads(CONFIG.read_text())


def sh(cmd, **kw):
    return subprocess.run(cmd, check=True, text=True, **kw)


# --- camera ---------------------------------------------------------------------------------


def adb(cfg, *args):
    serial = ["-s", cfg["phone"]] if "phone" in cfg else []
    return subprocess.run(["adb", *serial, *args], capture_output=True, text=True)


def camera_start(cfg):
    pidfile = HOME / "scrcpy.pid"
    if pidfile.exists() and Path(f"/proc/{pidfile.read_text().strip()}").exists():
        print("camera already running")
        return
    cam = cfg.get("camera", {})
    sink = cam.get("sink", "/dev/video10")
    args = cam.get(
        "scrcpy_args",
        "--video-source=camera --camera-facing=back --camera-size=1280x720 "
        "--camera-fps=15 --video-bit-rate=2M --no-audio",
    ).split()
    serial = ["-s", cfg["phone"]] if "phone" in cfg else []
    # Keep the phone awake while the feed runs; camera_stop restores the old setting.
    old = adb(cfg, "shell", "settings", "get", "global", "stay_on_while_plugged_in").stdout.strip()
    (HOME / "stayon.old").write_text(old or "0")
    adb(cfg, "shell", "svc", "power", "stayon", "true")
    log = open(HOME / "scrcpy.log", "w")
    proc = subprocess.Popen(
        ["scrcpy", *serial, *args, f"--v4l2-sink={sink}", "--no-window"],
        stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
    )
    pidfile.write_text(str(proc.pid))
    for _ in range(40):
        time.sleep(0.25)
        if proc.poll() is not None:
            die(f"scrcpy exited; see {HOME / 'scrcpy.log'}")
        if snap_frame(cfg, HOME / "snaps" / "camera-check.png", quiet=True):
            print(f"camera running (pid {proc.pid})")
            return
    die("no frame from the loopback device after 10s")


def camera_stop(cfg):
    pidfile = HOME / "scrcpy.pid"
    if pidfile.exists():
        try:
            os.killpg(int(pidfile.read_text()), signal.SIGTERM)
        except ProcessLookupError:
            pass
        pidfile.unlink()
    old = HOME / "stayon.old"
    if old.exists():
        adb(cfg, "shell", "settings", "put", "global", "stay_on_while_plugged_in", old.read_text())
        old.unlink()
    print("camera stopped")


# --- snapping and decoding ------------------------------------------------------------------


def snap_frame(cfg, out, quiet=False):
    """Grab one frame, rotate it to the device's orientation, and save it."""
    cam = cfg.get("camera", {})
    rotate = {0: [], 90: ["-vf", "transpose=1"], 180: ["-vf", "transpose=1,transpose=1"],
              270: ["-vf", "transpose=2"]}[cam.get("rotate", 90)]
    out.parent.mkdir(parents=True, exist_ok=True)
    r = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-f", "v4l2", "-i", cam.get("sink", "/dev/video10"),
         "-frames:v", "1", *rotate, "-y", str(out)],
        capture_output=True, text=True, timeout=15,
    )
    if r.returncode != 0 and not quiet:
        print(f"snap failed: {r.stderr.strip()}", file=sys.stderr)
    return r.returncode == 0


def decode(path):
    """Return (payload, polygon) for the first QR in the image, or (None, None)."""
    r = subprocess.run(
        ["zbarimg", "-q", "--raw", "--polygon", "-Sdisable", "-Sqrcode.enable", str(path)],
        capture_output=True, text=True,
    )
    if r.returncode != 0 or ":" not in r.stdout:
        return None, None
    points, payload = r.stdout.split(":", 1)
    polygon = [tuple(int(v) for v in p.split(",")) for p in points.split()]
    return payload.rstrip("\n"), polygon


def screen_box(payload, polygon):
    """Where the device screen sits in the frame, worked out from the QR's corners."""
    n = len(payload.encode())
    version = next((v + 1 for v, cap in enumerate(QR_CAPACITY_M) if cap >= n), None)
    if version is None or not polygon:
        return None
    modules = 17 + 4 * version
    scale = min(QR_AREA[2], QR_AREA[3]) // (modules + 2 * QUIET_ZONE)
    xs, ys = [p[0] for p in polygon], [p[1] for p in polygon]
    k = max(max(xs) - min(xs), max(ys) - min(ys)) / (modules * scale)
    cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    qr_cx, qr_cy = QR_AREA[0] + QR_AREA[2] / 2, QR_AREA[1] + QR_AREA[3] / 2
    margin = 10
    x0 = cx - (qr_cx + margin) * k
    y0 = cy - (qr_cy + margin) * k
    return [round(x0), round(y0), round((SCREEN[0] + 2 * margin) * k), round((SCREEN[1] + 2 * margin) * k)]


def crop(src, box, out):
    x, y, w, h = box
    r = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-i", str(src), "-vf",
         f"crop={w}:{h}:{max(x, 0)}:{max(y, 0)}", "-y", str(out)],
        capture_output=True, text=True,
    )
    return r.returncode == 0


def parse(payload):
    """`<run>|<STATE>|<beat>|<result>` from the card."""
    parts = payload.split("|", 3) if payload else []
    if len(parts) != 4 or not parts[2].isdigit():
        return None
    return {"run": parts[0], "state": parts[1], "beat": int(parts[2]), "result": parts[3]}


def snap(cfg, out):
    """Snap, decode and crop. Returns the parsed card, or None if no code was read."""
    if not snap_frame(cfg, out):
        return None
    payload, polygon = decode(out)
    card = parse(payload)
    box = screen_box(payload, polygon) if card else cfg.get("camera", {}).get("crop")
    if box:
        crop(out, box, out.with_name(out.stem + "-screen.png"))
    return card


# --- the run ---------------------------------------------------------------------------------


def device_mac(port):
    r = subprocess.run(["espflash", "board-info", "--no-stub", "-p", port],
                       capture_output=True, text=True, timeout=60)
    # board-info leaves the chip in download mode.
    subprocess.run(["espflash", "reset", "-p", port], capture_output=True, timeout=30)
    m = re.search(r"MAC address:\s*([0-9a-fA-F:]{17})", r.stdout + r.stderr)
    if not m:
        die(f"could not read the MAC on {port}:\n{r.stdout}{r.stderr}")
    return m.group(1).lower()


def check_device(cfg, port):
    allow = {m.lower() for m in cfg.get("allow", [])}
    mac = device_mac(port)
    if mac not in allow:
        die(f"{port} is {mac}, which is not on the allowlist in {CONFIG}")
    return mac


def snapshot():
    """The working tree, untracked files included, as a commit object no ref points at."""
    git = lambda *a, **kw: sh(["git", "-C", REPO, *a], capture_output=True, **kw).stdout.strip()
    env = {**os.environ, "GIT_INDEX_FILE": str(HOME / "snapshot.index")}
    git("read-tree", "HEAD", env=env)
    git("add", "-A", env=env)
    tree = git("write-tree", env=env)
    return git("commit-tree", tree, "-p", "HEAD", "-m", "hwlab snapshot")


def harness_patch(harness):
    """The harness as a patch, for specs built on another base."""
    base = sh(["git", "-C", REPO, "merge-base", harness, "origin/master"], capture_output=True).stdout.strip()
    return sh(["git", "-C", REPO, "diff", base, harness, "--", "device", "Cargo.lock"],
              capture_output=True).stdout


def prepare_tree(base, patches, run_dir):
    if not BUILD.exists():
        sh(["git", "-C", REPO, "worktree", "add", "--detach", BUILD, "HEAD"])
    sh(["git", "-C", BUILD, "checkout", "-q", "--detach", "-f", base])
    sh(["git", "-C", BUILD, "clean", "-q", "-fd"])  # keeps the ignored target dir
    for name, text in patches:
        if not text.strip():
            continue
        (run_dir / name).write_text(text)
        r = subprocess.run(["git", "-C", BUILD, "apply", "--3way", "--whitespace=nowarn", run_dir / name],
                           capture_output=True, text=True)
        if r.returncode != 0:
            die(f"{name} does not apply on {base}:\n{r.stderr}")


def build_and_flash(env, port, run_dir):
    with open(run_dir / "build.log", "w") as log:
        for cmd in (["just", "env=dev", "build-firmware-signed", "frontier"],
                    ["just", "env=dev", "flash-firmware"]):
            print(f"  {' '.join(cmd)}", flush=True)
            r = subprocess.run(cmd, cwd=BUILD, env=env, stdout=log, stderr=subprocess.STDOUT)
            if r.returncode != 0:
                die(f"`{' '.join(cmd)}` failed; see {run_dir / 'build.log'}")
    # write-bin leaves the chip in the bootloader on some setups; a reset is harmless otherwise.
    subprocess.run(["espflash", "reset", "-p", port], capture_output=True, timeout=30)


def watch(cfg, run, spec, run_dir):
    interval = spec.get("snap_secs", 3)
    timeout = spec.get("timeout_secs", 300)
    stall = spec.get("stall_secs", 30)
    frames = run_dir / "frames"
    start = time.monotonic()
    last_beat, last_change, last_card = None, start, None
    stats = {"snaps": 0, "read": 0}
    i = 0
    with open(run_dir / "snaps.jsonl", "w") as log:
        while True:
            now = time.monotonic()
            if now - start > timeout:
                return "TIMEOUT", last_card, stats
            card = snap(cfg, frames / f"{i:04}.png")
            log.write(json.dumps({"t": round(now - start, 1), "frame": i, "card": card}) + "\n")
            log.flush()
            i += 1
            stats["snaps"] += 1
            if card is not None and card["run"] == run:
                stats["read"] += 1
                last_card = card
                if card["state"] in ("DONE", "PANIC"):
                    return card["state"], card, stats
                if card["beat"] != last_beat:
                    last_beat, last_change = card["beat"], now
            if last_card and now - last_change > stall:
                return "STALL", last_card, stats
            time.sleep(interval)


def run_variant(cfg, spec_path, spec, variant, port, overrides):
    v = spec["variants"][variant]
    params = {**spec.get("params", {}), **v.get("params", {}), **overrides}
    for name, limit in spec.get("limits", {}).items():
        if name in params and int(params[name]) > int(limit):
            die(f"{name}={params[name]} is over the spec's limit of {limit}")

    run = secrets.token_hex(3)
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    run_dir = RUNS / f"{stamp}-{spec['id']}-{variant}-{run}"
    run_dir.mkdir(parents=True)
    shutil.copy(spec_path, run_dir / "spec.toml")

    harness = snapshot()
    base = spec.get("base")
    base_sha = sh(["git", "-C", REPO, "rev-parse", base], capture_output=True).stdout.strip() if base else harness
    patches = []
    if base_sha != harness:
        patches.append(("harness.patch", harness_patch(harness)))
    if "patch" in v:
        text = (spec_path.parent / v["patch"]).read_text()
        if FORBIDDEN_IN_PATCH.search(text):
            die(f"{v['patch']} touches eFuse programming or provisioning; refusing")
        patches.append(("experiment.patch", text))
    prepare_tree(base_sha, patches, run_dir)

    env = {**os.environ, "PATH": f"{RISCV_BIN}:{os.environ['PATH']}", "ESPFLASH_PORT": port,
           "DEVICE_BUILD_ARGS": "--features hwlab", "HWLAB_ID": f"{spec['id']}/{variant}"[:22],
           "HWLAB_RUN": run, **{f"HWLAB_{k}": str(val) for k, val in params.items()}}
    meta = {"run": run, "spec": spec["id"], "variant": variant, "params": params, "base": base_sha,
            "harness": harness, "port": port, "started": stamp}
    (run_dir / "meta.json").write_text(json.dumps(meta, indent=2))

    print(f"[{run}] {spec['id']}/{variant} {params} on {base_sha[:8]}", flush=True)
    build_and_flash(env, port, run_dir)
    outcome, card, stats = watch(cfg, run, spec, run_dir)
    meta.update(outcome=outcome, card=card, snaps=stats, finished=datetime.datetime.now().isoformat())
    (run_dir / "meta.json").write_text(json.dumps(meta, indent=2))
    result = card["result"] if card else "-"
    beat = card["beat"] if card else "-"
    print(f"RESULT {run} {spec['id']}/{variant} {outcome} beat={beat} read={stats['read']}/{stats['snaps']} "
          f"{result} ({run_dir})", flush=True)


def cmd_run(args):
    cfg = config()
    spec_path = Path(args.spec).resolve()
    spec = tomllib.loads(spec_path.read_text())
    variants = args.variants or list(spec["variants"])
    if "control" not in spec["variants"]:
        die("every spec needs a control variant")
    port = args.port or cfg.get("port") or die("no port: pass --port or set port in config")
    mac = check_device(cfg, port)
    print(f"device {mac} on {port}", flush=True)
    overrides = dict(kv.split("=", 1) for kv in args.set)
    for variant in variants:
        run_variant(cfg, spec_path, spec, variant, port, overrides)


def cmd_calibrate(args):
    cfg = config()
    out = HOME / "calibration" / datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    ok, beats, wrong = 0, set(), 0
    for i in range(args.n):
        card = snap(cfg, out / f"{i:04}.png")
        if card:
            ok += 1
            beats.add(card["beat"])
            m = re.fullmatch(r"cal (\d+) ([a-z]*)", card["result"])
            expected = "".join(chr(ord("a") + j % 26) for j in range(int(m.group(1)))) if m else None
            if not m or m.group(2) != expected:
                wrong += 1
        time.sleep(args.interval)
    print(f"decoded {ok}/{args.n} ({100 * ok / args.n:.0f}%), {len(beats)} distinct beats, "
          f"{wrong} decoded but wrong, frames in {out}")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("camera")
    c.add_argument("action", choices=["start", "stop"])
    s = sub.add_parser("snap")
    s.add_argument("name", nargs="?", default="snap")
    k = sub.add_parser("calibrate")
    k.add_argument("n", nargs="?", type=int, default=100)
    k.add_argument("--interval", type=float, default=0.5)
    r = sub.add_parser("run")
    r.add_argument("spec")
    r.add_argument("variants", nargs="*")
    r.add_argument("--port")
    r.add_argument("--set", action="append", default=[], metavar="NAME=VALUE")
    args = p.parse_args()

    HOME.mkdir(parents=True, exist_ok=True)
    if args.cmd == "camera":
        (camera_start if args.action == "start" else camera_stop)(config())
    elif args.cmd == "snap":
        out = HOME / "snaps" / f"{args.name}.png"
        card = snap(config(), out)
        print(json.dumps(card) if card else "no code read", out)
    elif args.cmd == "calibrate":
        cmd_calibrate(args)
    elif args.cmd == "run":
        cmd_run(args)


if __name__ == "__main__":
    main()
