import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("store", ROOT / "scripts/app-store-ios.py")
store = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(store)
NOTES = json.loads((ROOT / "docs/store/ios-0.5.1.json").read_text())


class InheritedListing(store.AppStore):
    def __init__(self, mismatch):
        self.mismatch = mismatch
        self.mutations = []

    def ios_versions(self):
        return [
            {"id": "older", "attributes": {"versionString": "0.4.0", "appVersionState": "READY_FOR_DISTRIBUTION"}},
            {"id": "old", "attributes": {"versionString": "0.5.0", "appVersionState": "READY_FOR_DISTRIBUTION"}},
            {"id": "new", "attributes": {"versionString": "0.5.1", "appVersionState": "PREPARE_FOR_SUBMISSION"}},
        ]

    def collection(self, path, query=None):
        if "/older/" in path:
            raise AssertionError("Must inherit the latest published version")
        is_new = "/new/" in path
        result = []
        for locale in NOTES["whatsNew"]:
            attrs = {"locale": locale, "description": "Published description", "supportUrl": "https://ls.mixduo.cn/support"}
            if is_new and self.mismatch == "description":
                attrs["description"] = "Unexpected replacement"
            result.append({"id": ("new-" if is_new else "old-") + locale, "attributes": attrs})
        return result[:-1] if is_new and self.mismatch == "locale" else result

    def screenshots(self, identifier):
        checksum = "changed" if identifier.startswith("new-") and self.mismatch == "screenshot" else "published"
        return {"APP_IPHONE_67": [("screen.png", checksum, 12345)]}

    def request(self, method, path, **kwargs):
        self.mutations.append((method, path))
        raise AssertionError("Listing mismatch must stop before mutations")


class ReleaseGuards(unittest.TestCase):
    def test_inherited_listing_changes_stop_before_mutations(self):
        for mismatch in ("locale", "description", "screenshot"):
            with self.subTest(mismatch=mismatch):
                fixture = InheritedListing(mismatch)
                with self.assertRaises(RuntimeError):
                    fixture.prepare("0.5.1")
                self.assertEqual(fixture.mutations, [])

    def test_invalid_or_wrong_build_is_refused(self):
        valid = {"version": "26", "processingState": "VALID", "expired": False, "usesNonExemptEncryption": False}
        store.validate_build({"attributes": valid}, "26")
        for field, value in (("version", "25"), ("processingState", "PROCESSING"),
                             ("expired", True), ("usesNonExemptEncryption", None),
                             ("usesNonExemptEncryption", True)):
            with self.subTest(field=field, value=value), self.assertRaises(RuntimeError):
                store.validate_build({"attributes": {**valid, field: value}}, "26")


class LocalScreenshots(unittest.TestCase):
    def test_refreshed_screenshots_replace_only_the_matching_display_type(self):
        fixture = InheritedListing(None)
        existing = {"APP_IPHONE_67": [("old.png", "x", 1)], "APP_IPAD_PRO_3GEN_129": [("old.png", "y", 2)],
                    "APP_IPHONE_40": [("keep.png", "z", 3)]}
        result = fixture.local_screenshots(existing)
        self.assertEqual(result["APP_IPHONE_40"], existing["APP_IPHONE_40"])
        self.assertEqual([n for n, _, _ in result["APP_IPHONE_67"]],
                         sorted(p.name for p in (ROOT / "apps/app/store/screenshots/iphone-6.5").glob("*.png")))
        self.assertTrue(all(len(c) == 32 for _, c, _ in result["APP_IPHONE_67"]))

    def test_macos_listing_uses_the_desktop_display_type(self):
        fixture = InheritedListing(None)
        fixture.platform = "MAC_OS"
        result = fixture.local_screenshots({"APP_DESKTOP": [("old.png", "x", 1)]})
        self.assertEqual(len(result["APP_DESKTOP"]), 4)


if __name__ == "__main__":
    unittest.main()
