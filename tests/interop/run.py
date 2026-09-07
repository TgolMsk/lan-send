#!/usr/bin/env python3
"""Interoperability test between lan-send and the official LocalSend CLI.

Two directions are exercised on one machine, each side with its own config
directory and HTTP port (the multicast port 53317 is shared):

  A. official -> ours   `localsend-cli send --to <alias>`  vs  `lan-send receive --auto-accept`
  B. ours -> official   `lan-send send <alias>`             vs  `localsend-cli` (interactive, pre-paired)

The official binary is downloaded from GitHub Releases into `.cache/` when
`--official` is not given. Both directions drive the official TUI with `pexpect` (pip install pexpect).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request
from pathlib import Path

OFFICIAL_VERSION = "1.18.2"
OURS_PORT = 53400
OFFICIAL_PORT = 53401
PAYLOAD_SIZE = 3 * 1024 * 1024 + 12345
HERE = Path(__file__).resolve().parent
CACHE = HERE / ".cache"


def official_asset() -> str:
    system = platform.system()
    machine = platform.machine().lower()
    arch = "arm-64" if machine in ("arm64", "aarch64") else "x86-64"
    if system == "Darwin":
        return f"LocalSend-CLI-{OFFICIAL_VERSION}-macos-{arch}.tar.gz"
    if system == "Linux":
        return f"LocalSend-CLI-{OFFICIAL_VERSION}-linux-{arch}.tar.gz"
    if system == "Windows":
        return f"LocalSend-CLI-{OFFICIAL_VERSION}-windows-{arch}.exe"
    raise SystemExit(f"unsupported platform {system}/{machine}")


def ensure_official() -> Path:
    asset = official_asset()
    CACHE.mkdir(parents=True, exist_ok=True)
    if asset.endswith(".exe"):
        binary = CACHE / "localsend-cli.exe"
    else:
        binary = CACHE / "localsend-cli"
    if binary.exists():
        return binary
    url = f"https://github.com/localsend/localsend/releases/download/v{OFFICIAL_VERSION}/{asset}"
    archive = CACHE / asset
    print(f"downloading {url}")
    urllib.request.urlretrieve(url, archive)
    if asset.endswith(".exe"):
        archive.rename(binary)
    else:
        with tarfile.open(archive) as tar:
            tar.extractall(CACHE)
        candidates = [p for p in CACHE.rglob("localsend-cli*") if p.is_file() and p != archive]
        if not candidates:
            raise SystemExit("localsend-cli not found in the archive")
        if candidates[0] != binary:
            shutil.copy2(candidates[0], binary)
        binary.chmod(0o755)
    return binary


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def make_payload(path: Path) -> str:
    with path.open("wb") as handle:
        handle.write(os.urandom(PAYLOAD_SIZE))
    return sha256(path)


def wait_for_file(directory: Path, name: str, expected: str, timeout: float) -> Path:
    deadline = time.time() + timeout
    while time.time() < deadline:
        for candidate in directory.rglob("*"):
            if candidate.is_file() and candidate.name == name and sha256(candidate) == expected:
                return candidate
        time.sleep(0.5)
    raise AssertionError(f"{name} did not arrive in {directory} within {timeout}s")


def wait_for_text(log: Path, needle: str, timeout: float) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if log.exists() and needle in log.read_text(errors="replace"):
            return
        time.sleep(0.2)
    raise AssertionError(f"'{needle}' not seen in {log} within {timeout}s")


def terminate(process: subprocess.Popen) -> None:
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()


def official_env(config_home: Path) -> dict:
    env = dict(os.environ)
    env["XDG_CONFIG_HOME"] = str(config_home)
    return env


def strip_ansi(text: str) -> str:
    return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07]*\x07|\x1b[=>]", "", text)


def test_official_to_ours(ours: Path, official: Path, tmp: Path) -> None:
    """The released official CLI has no headless send: its TUI lists the
    discovered devices as `[n] alias (...)` and the digit key sends `-f`
    files to that device."""
    print("\n== A. official -> ours ==")
    import pexpect  # type: ignore

    ours_cfg, official_cfg, inbox = tmp / "ours-cfg", tmp / "official-cfg", tmp / "inbox"
    inbox.mkdir()
    payload = tmp / "payload-a.bin"
    expected = make_payload(payload)
    log = tmp / "ours-receive.log"

    with log.open("wb") as log_handle:
        receiver = subprocess.Popen(
            [str(ours), "--config-dir", str(ours_cfg), "--alias", "ours-recv", "--port", str(OURS_PORT),
             "-vv", "receive", "--dir", str(inbox), "--auto-accept"],
            stdout=log_handle, stderr=subprocess.STDOUT,
        )
        child = None
        try:
            wait_for_text(log, "Receiving as ours-recv", 15)
            child = pexpect.spawn(
                str(official),
                ["--alias", "official", "--port", str(OFFICIAL_PORT), "--destination", str(tmp),
                 "-f", str(payload)],
                env=official_env(official_cfg), encoding="utf-8", timeout=45, dimensions=(30, 120),
            )
            child.expect(r"\[(\d)\] ours-recv")
            number = child.match.group(1)
            # `-f` opens the device list with the first entry selected; Enter
            # sends to it. The digit hotkey only works on the main screen.
            print(f"official lists ours-recv as [{number}]; pressing Enter")
            child.send("\r")
            try:
                received = wait_for_file(inbox, payload.name, expected, 30)
            except AssertionError:
                print("no transfer after Enter; trying the digit hotkey")
                child.send("\x1b")
                time.sleep(0.5)
                child.send(number)
                received = wait_for_file(inbox, payload.name, expected, 30)
            print(f"received {received} OK")
        finally:
            if child is not None:
                try:
                    child.sendintr()
                    child.expect(pexpect.EOF, timeout=10)
                except Exception:  # noqa: BLE001 - best effort shutdown
                    child.close(force=True)
                print("official output tail:\n" + strip_ansi(child.before or "")[-800:])
            terminate(receiver)
    print(log.read_text(errors="replace")[-1500:])


def our_fingerprint(ours: Path, config_dir: Path) -> str:
    output = subprocess.run(
        [str(ours), "--config-dir", str(config_dir), "identity"],
        capture_output=True, text=True, check=True,
    ).stdout
    for line in output.splitlines():
        if line.startswith("Fingerprint:"):
            return line.split(":", 1)[1].strip()
    raise AssertionError(f"no fingerprint in:\n{output}")


def test_ours_to_official(ours: Path, official: Path, tmp: Path) -> None:
    print("\n== B. ours -> official ==")
    import pexpect  # type: ignore

    ours_cfg, official_cfg, inbox = tmp / "ours-cfg-b", tmp / "official-cfg-b", tmp / "inbox-b"
    inbox.mkdir()
    payload = tmp / "payload-b.bin"
    expected = make_payload(payload)

    # Pre-pair our device on the official side so it auto-accepts.
    fingerprint = our_fingerprint(ours, ours_cfg)
    paired_dir = official_cfg / "localsend-cli"
    paired_dir.mkdir(parents=True)
    (paired_dir / "paired-v2.json").write_text(json.dumps({
        "version": 1,
        "devices": {fingerprint: {"alias": "ours-send", "channels": []}},
    }))

    child = pexpect.spawn(
        str(official),
        ["--alias", "official-b", "--port", str(OFFICIAL_PORT), "--destination", str(inbox)],
        env=official_env(official_cfg), encoding="utf-8", timeout=30, dimensions=(30, 120),
    )
    try:
        time.sleep(3)  # let the official server and discovery come up
        sender = subprocess.run(
            [str(ours), "--config-dir", str(ours_cfg), "--alias", "ours-send", "--port", str(OURS_PORT),
             "send", "official-b", str(payload), "--timeout", "20"],
            capture_output=True, text=True, timeout=180,
        )
        print(sender.stdout[-2000:])
        print(sender.stderr[-2000:])
        if sender.returncode != 0:
            raise AssertionError(f"lan-send send exited with {sender.returncode}")
        received = wait_for_file(inbox, payload.name, expected, 30)
        print(f"received {received} OK")
    finally:
        try:
            child.sendintr()
            child.expect(pexpect.EOF, timeout=10)
        except Exception:  # noqa: BLE001 - best effort shutdown
            child.close(force=True)
        print("official output tail:\n" + strip_ansi(child.before or "")[-800:])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--ours", type=Path, default=HERE.parent.parent / "target" / "debug" / "lan-send")
    parser.add_argument("--official", type=Path, default=None)
    parser.add_argument("--only", choices=["a", "b"], default=None)
    parser.add_argument("--keep", action="store_true", help="keep the temporary directory")
    args = parser.parse_args()

    ours = args.ours.resolve()
    if not ours.exists():
        raise SystemExit(f"{ours} does not exist; build with `cargo build -p lan-send-cli`")
    official = (args.official or ensure_official()).resolve()
    print(f"ours:     {ours}\nofficial: {official}")

    tmp = Path(tempfile.mkdtemp(prefix="lan-send-interop-"))
    failures = 0
    try:
        for name, test in (("a", test_official_to_ours), ("b", test_ours_to_official)):
            if args.only and args.only != name:
                continue
            try:
                test(ours, official, tmp)
                print(f"PASS {name}")
            except Exception as error:  # noqa: BLE001 - report and continue
                failures += 1
                print(f"FAIL {name}: {error}")
    finally:
        if args.keep:
            print(f"kept {tmp}")
        else:
            shutil.rmtree(tmp, ignore_errors=True)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
