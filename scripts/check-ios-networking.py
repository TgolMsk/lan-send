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
    check_scene_lifecycle(info, label)


def check_scene_lifecycle(info, label):
    # The iOS 27 SDK traps during UIApplicationMain without a static scene
    # configuration, before any Rust network code or Tauri UI can run.
    manifest = info.get("UIApplicationSceneManifest", {})
    if not isinstance(manifest, dict):
        raise ValueError(f"{label}: invalid scene manifest")
    configurations = manifest.get("UISceneConfigurations", {})
    scenes = configurations.get("UIWindowSceneSessionRoleApplication", []) if isinstance(configurations, dict) else []
    if not isinstance(scenes, list) or not any(
        isinstance(scene, dict)
        and scene.get("UISceneConfigurationName")
        and scene.get("UISceneDelegateClassName") == "TaoSceneDelegate"
        for scene in scenes
    ):
        raise ValueError(f"{label}: missing TaoSceneDelegate scene configuration (iOS 27 launch crash)")
    if manifest.get("UIApplicationSupportsMultipleScenes") is not False:
        raise ValueError(f"{label}: LanSend must retain its single-window scene configuration")


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
    generated_info = plistlib.loads(
        (TAURI / "gen/apple/lan-send-app_iOS/Info.plist").read_bytes()
    )
    check_scene_lifecycle(generated_info, "generated iOS source")
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
    print("iOS networking permissions and scene lifecycle OK" + (" (signature and profile)" if args.app or args.ipa else " (source)"))


if __name__ == "__main__":
    main()
