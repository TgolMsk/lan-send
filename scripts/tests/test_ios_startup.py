"""Reject the missing/empty scene manifest that crashed TestFlight build 26."""

import copy
import importlib.util
from pathlib import Path
import plistlib
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "check-ios-networking.py"
SPEC = importlib.util.spec_from_file_location("ios_network_check", SCRIPT)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class SceneStartupTests(unittest.TestCase):
    def setUp(self):
        self.info = plistlib.loads((CHECK.TAURI / "Info.ios.plist").read_bytes())

    def test_rejects_legacy_and_incomplete_manifests(self):
        variants = [{}, {"UIApplicationSceneManifest": {}},
                    {"UIApplicationSceneManifest": {"UISceneConfigurations": {}}}]
        for info in variants:
            with self.subTest(info=info), self.assertRaisesRegex(ValueError, "scene configuration"):
                CHECK.check_scene_lifecycle(info, "regression")

    def test_rejects_missing_native_delegate(self):
        self.info["UIApplicationSceneManifest"]["UISceneConfigurations"]["UIWindowSceneSessionRoleApplication"][0]["UISceneDelegateClassName"] = "SceneDelegate"
        with self.assertRaisesRegex(ValueError, "TaoSceneDelegate"):
            CHECK.check_scene_lifecycle(self.info, "regression")

    def test_source_declares_single_scene(self):
        CHECK.check_scene_lifecycle(self.info, "source")
        broken = copy.deepcopy(self.info)
        broken["UIApplicationSceneManifest"]["UIApplicationSupportsMultipleScenes"] = True
        with self.assertRaisesRegex(ValueError, "single-window"):
            CHECK.check_scene_lifecycle(broken, "regression")


if __name__ == "__main__":
    unittest.main()
