"""Reference memory dumps from MAME's ``arcadia`` driver.

MAME is taken from ``PATH`` (or ``/usr/games/mame``); ``PYVACS_MAME_DOCKER=<image>``
runs it in that Docker image instead, bind-mounting the work directory at ``/w``.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Tuple

import numpy as np

UVI_BASE = 0x1800
UVI_SIZE = 0x300
SDL_ENV = {"SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy"}

LUA = """\
frames = 0
want = {{{frames}}}
start = manager.machine.ioport.ports[":panel"].fields["Start"]
sub = emu.add_machine_frame_notifier(function()
    frames = frames + 1
    if frames == {press_on} then start:set_value(1) end
    if frames == {press_off} then start:set_value(0) end
    if want[frames] then
        local sp = manager.machine.devices[":maincpu"].spaces["program"]
        local f = io.open("{out}/dump_" .. frames .. ".bin", "wb")
        for a = {base}, {top} do f:write(string.char(sp:read_u8(a))) end
        f:close()
    end
    if frames >= {last} then manager.machine:exit() end
end)
"""


def mame_command(work: Path) -> Optional[List[str]]:
    """The argv prefix that runs MAME with ``work`` visible, or None if unavailable."""
    image = os.environ.get("PYVACS_MAME_DOCKER")
    if image:
        env = [x for k, v in SDL_ENV.items() for x in ("-e", f"{k}={v}")]
        mount = [
            "-e",
            "HOME=/w",
            "-v",
            f"{work}:/w",
            f"--user={os.getuid()}:{os.getgid()}",
        ]
        return ["docker", "run", "--rm", "--init", *env, *mount, image, "mame"]
    found = shutil.which("mame") or shutil.which("/usr/games/mame")
    return [found] if found else None


def dump_uvi(
    cart: bytes,
    frames: Iterable[int],
    work: Path,
    start: Tuple[int, int] = (-1, -1),
    timeout: int = 120,
) -> Dict[int, np.ndarray]:
    """Run ``cart`` in MAME and return ``{frame: uint8[0x300]}`` UVI-window dumps.

    ``start`` holds the console Start button from frame ``start[0]`` to ``start[1]``.
    """
    work = work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    prefix = mame_command(work)
    if prefix is None:
        raise FileNotFoundError("mame not available")
    frames = sorted(set(frames))
    root = "/w" if prefix[0] == "docker" else str(work)
    (work / "cart.bin").write_bytes(cart)
    (work / "dump.lua").write_text(
        LUA.format(
            frames=", ".join(f"[{f}]=true" for f in frames),
            last=frames[-1],
            press_on=start[0],
            press_off=start[1],
            out=root,
            base=UVI_BASE,
            top=UVI_BASE + UVI_SIZE - 1,
        )
    )
    opts = {
        "-cart": f"{root}/cart.bin",
        "-sound": "none",
        "-video": "none",
        "-seconds_to_run": str(frames[-1] // 50 + 10),
        "-autoboot_script": f"{root}/dump.lua",
        "-cfg_directory": root,
        "-nvram_directory": root,
    }
    argv = prefix + ["arcadia", "-skip_gameinfo", "-nothrottle"]
    argv += [x for kv in opts.items() for x in kv]
    result = subprocess.run(
        argv,
        env=dict(os.environ, HOME=str(work), **SDL_ENV),
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    dumps = {}
    for frame in frames:
        path = work / f"dump_{frame}.bin"
        if not path.exists():
            raise RuntimeError(
                f"MAME produced no dump for frame {frame} (rc={result.returncode})\n"
                f"{result.stdout}\n{result.stderr}"
            )
        dumps[frame] = np.frombuffer(path.read_bytes(), dtype=np.uint8)
        path.unlink()
    return dumps


def settled(dumps: Iterable[np.ndarray]) -> np.ndarray:
    """Mask of bytes equal across every dump: content that has stopped changing."""
    stack = np.stack(list(dumps))
    return (stack == stack[0]).all(axis=0)
