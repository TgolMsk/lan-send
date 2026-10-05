#!/usr/bin/env python3
"""Check source permissions, or an exported signed iOS app before upload."""

import argparse
from pathlib import Path
import plistlib
import subprocess
import tempfile
import zipfile

MULTICAST = "com.apple.developer.networking.multicast"
TAURI = Path(__file__).resolve().parents[1] / "apps/app/src-tauri"


def check_permissions(info, entitlements, label):
    if not info.get("NSLocalNetworkUsageDescription", "").strip():
        raise ValueError(f"{label}: missing local-network usage description")
    if entitlements.get(MULTICAST) is not True:
        raise ValueError(f"{label}: missing multicast networking entitlement")


def check_app(app):
    info = plistlib.loads((app / "Info.plist").read_bytes())
    signed = subprocess.check_output(
        ["codesign", "-d", "--entitlements", ":-", str(app)],
        stderr=subprocess.DEVNULL,
    )
    check_permissions(info, plistlib.loads(signed), "signed app")
    profile = plistlib.loads(
        subprocess.check_output(
            ["security", "cms", "-D", "-i", str(app / "embedded.mobileprovision")],
            stderr=subprocess.DEVNULL,
        )
    )
    if profile.get("Entitlements", {}).get(MULTICAST) is not True:
        raise ValueError("provisioning profile: missing multicast networking capability")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    artifacts = parser.add_mutually_exclusive_group()
    artifacts.add_argument("--app", type=Path)
    artifacts.add_argument("--ipa", type=Path)
    args = parser.parse_args()
    info = plistlib.loads((TAURI / "Info.ios.plist").read_bytes())
    entitlements = plistlib.loads(
        (TAURI / "gen/apple/lan-send-app_iOS/lan-send-app_iOS.entitlements").read_bytes()
    )
    check_permissions(info, entitlements, "iOS source")
    if args.app:
        check_app(args.app)
    elif args.ipa:
        with tempfile.TemporaryDirectory(prefix="lansend-ios-network-") as directory:
            with zipfile.ZipFile(args.ipa) as archive:
                archive.extractall(directory)
            apps = list((Path(directory) / "Payload").glob("*.app"))
            if len(apps) != 1:
                raise ValueError("IPA must contain exactly one application")
            check_app(apps[0])
    print("iOS networking permissions OK" + (" (signature and profile)" if args.app or args.ipa else " (source)"))


if __name__ == "__main__":
    main()
