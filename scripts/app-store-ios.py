#!/usr/bin/env python3
"""Manage this app's iOS release using ASC credentials kept on the CI runner."""
import argparse
import json
import os
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request

APP_ID = "6809459213"
BUNDLE_ID = "com.wangsheng.lansend"
BASE = "https://api.appstoreconnect.apple.com"


class AppStore:
    def request(self, method, path, *, query=None, data=None, missing_ok=False):
        import jwt

        now = int(time.time())
        token = jwt.encode(
            {"iss": os.environ["APPLE_API_ISSUER"], "iat": now,
             "exp": now + 600, "aud": "appstoreconnect-v1"},
            os.environ["APPSTORE_PRIVATE_KEY"], algorithm="ES256",
            headers={"kid": os.environ["APPLE_API_KEY"], "typ": "JWT"},
        )
        url = path if path.startswith(BASE + "/") else BASE + path
        if not url.startswith(BASE + "/"):
            raise ValueError("Unexpected API URL")
        if query:
            url += "?" + urllib.parse.urlencode(query)
        req = urllib.request.Request(
            url, method=method,
            headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
            data=None if data is None else json.dumps(data).encode(),
        )
        try:
            with urllib.request.urlopen(req, timeout=60) as response:
                body = response.read()
                return json.loads(body) if body else {}
        except urllib.error.HTTPError as error:
            if missing_ok and error.code == 404:
                return None
            errors = json.loads(error.read()).get("errors", [])
            raise RuntimeError(f"ASC {method} {path}: HTTP {error.code}: " +
                               "; ".join(e.get("detail", e.get("title", "")) for e in errors)) from None

    def collection(self, path, query=None):
        result = []
        while path:
            response = self.request("GET", path, query=query)
            result.extend(response["data"])
            path = response.get("links", {}).get("next")
            query = None
        return result

    def inspect(self):
        app = self.request("GET", f"/v1/apps/{APP_ID}")["data"]
        if app["attributes"]["bundleId"] != BUNDLE_ID:
            raise RuntimeError("App ID does not match the LanSend bundle identifier")
        versions = self.collection(f"/v1/apps/{APP_ID}/appStoreVersions",
                                   {"filter[platform]": "IOS", "limit": 200})
        builds = self.request("GET", "/v1/builds", query={
            "filter[app]": APP_ID, "filter[preReleaseVersion.platform]": "IOS",
            "sort": "-uploadedDate", "limit": 10, "include": "preReleaseVersion",
        })["data"]
        print(json.dumps({"app": APP_ID, "versions": [
            {"id": v["id"], **v["attributes"]} for v in versions
        ], "builds": [{"id": b["id"], **{k: b["attributes"].get(k) for k in
            ("version", "uploadedDate", "processingState", "usesNonExemptEncryption")}}
            for b in builds]},
            ensure_ascii=False, indent=2))

    def ios_versions(self):
        app = self.request("GET", f"/v1/apps/{APP_ID}")["data"]
        if app["attributes"]["bundleId"] != BUNDLE_ID:
            raise RuntimeError("Unexpected bundle identifier")
        return self.collection(f"/v1/apps/{APP_ID}/appStoreVersions",
                               {"filter[platform]": "IOS", "limit": 200})

    def screenshots(self, localization_id):
        sets = self.collection(f"/v1/appStoreVersionLocalizations/{localization_id}/appScreenshotSets")
        result = {}
        for asset_set in sets:
            assets = self.collection(f"/v1/appScreenshotSets/{asset_set['id']}/appScreenshots")
            result[asset_set["attributes"]["screenshotDisplayType"]] = [
                (a["attributes"].get("fileName"), a["attributes"].get("sourceFileChecksum"),
                 a["attributes"].get("fileSize")) for a in assets
            ]
        return result

    def prepare(self, version):
        notes = json.loads((Path(__file__).resolve().parent.parent /
                            f"docs/store/ios-{version}.json").read_text())
        if notes["version"] != version:
            raise RuntimeError("Release notes version mismatch")
        versions = self.ios_versions()
        previous = [v for v in versions if version_state(v) in
                    ("READY_FOR_SALE", "READY_FOR_DISTRIBUTION")]
        # ASC retains READY_FOR_DISTRIBUTION on historical releases as well.
        # Choose the greatest numeric version instead of assuming one row.
        if not previous:
            raise RuntimeError("No published iOS version to inherit")
        previous = max(previous, key=lambda v: version_number(v["attributes"]["versionString"]))
        if version_number(version) <= version_number(previous["attributes"]["versionString"]):
            raise RuntimeError("New version must be newer than the published version")
        target = [v for v in versions if v["attributes"]["versionString"] == version]
        if not target:
            # ASC carries forward the published version's listing and assets.
            # Verify the inherited data below before touching the review queue.
            attrs = {"platform": "IOS", "versionString": version,
                     "releaseType": previous["attributes"]["releaseType"],
                     "copyright": previous["attributes"].get("copyright", "© 2026 Wang Sheng")}
            target = [self.request("POST", "/v1/appStoreVersions", data={"data": {
                "type": "appStoreVersions", "attributes": attrs,
                "relationships": {"app": relation("apps", APP_ID)},
            }})["data"]]
        if len(target) != 1:
            raise RuntimeError("Ambiguous iOS version")
        target = target[0]
        if version_state(target) != "PREPARE_FOR_SUBMISSION":
            raise RuntimeError(f"Version is not editable: {version_state(target)}")
        old_localizations = self.collection(
            f"/v1/appStoreVersions/{previous['id']}/appStoreVersionLocalizations")
        new_localizations = self.collection(
            f"/v1/appStoreVersions/{target['id']}/appStoreVersionLocalizations")
        old_by_locale = {v["attributes"]["locale"]: v for v in old_localizations}
        new_by_locale = {v["attributes"]["locale"]: v for v in new_localizations}
        if set(old_by_locale) != set(new_by_locale) or set(new_by_locale) != set(notes["whatsNew"]):
            raise RuntimeError("Inherited listing locales do not match the release notes")
        for locale, old in old_by_locale.items():
            new = new_by_locale[locale]
            for field in ("description", "keywords", "supportUrl", "marketingUrl", "promotionalText"):
                if old["attributes"].get(field) != new["attributes"].get(field):
                    raise RuntimeError(f"Inherited {locale} {field} differs from the published version")
            if self.screenshots(old["id"]) != self.screenshots(new["id"]):
                raise RuntimeError(f"Inherited screenshots differ for {locale}")
        for locale, new in new_by_locale.items():
            self.request("PATCH", f"/v1/appStoreVersionLocalizations/{new['id']}", data={"data": {
                "type": "appStoreVersionLocalizations", "id": new["id"],
                "attributes": {"whatsNew": notes["whatsNew"][locale]},
            }})
        old_review = self.request("GET", f"/v1/appStoreVersions/{previous['id']}/appStoreReviewDetail")["data"]
        new_review = self.request("GET", f"/v1/appStoreVersions/{target['id']}/appStoreReviewDetail", missing_ok=True)
        attrs = {k: v for k, v in old_review["attributes"].items() if k in (
            "contactEmail", "contactFirstName", "contactLastName", "contactPhone",
            "demoAccountRequired", "demoAccountName", "demoAccountPassword") and v is not None}
        attrs["notes"] = notes["reviewNotes"]
        if new_review:
            rid = new_review["data"]["id"]
            self.request("PATCH", f"/v1/appStoreReviewDetails/{rid}", data={"data": {
                "type": "appStoreReviewDetails", "id": rid, "attributes": attrs,
            }})
        else:
            self.request("POST", "/v1/appStoreReviewDetails", data={"data": {
                "type": "appStoreReviewDetails", "attributes": attrs,
                "relationships": {"appStoreVersion": relation("appStoreVersions", target["id"])},
            }})
        persisted = self.collection(f"/v1/appStoreVersions/{target['id']}/appStoreVersionLocalizations")
        if any(v["attributes"]["whatsNew"] != notes["whatsNew"][v["attributes"]["locale"]] for v in persisted):
            raise RuntimeError("Release notes did not persist")
        review = self.request("GET", f"/v1/appStoreVersions/{target['id']}/appStoreReviewDetail")["data"]
        if any(review["attributes"].get(k) != v for k, v in attrs.items()):
            raise RuntimeError("Review information did not persist")
        print(json.dumps({"version": version, "id": target["id"], "state": version_state(target),
                          "locales": sorted(new_by_locale), "screenshots": "unchanged"}))
        builds = self.collection("/v1/builds", {
            "filter[app]": APP_ID, "filter[preReleaseVersion.version]": version,
            "filter[preReleaseVersion.platform]": "IOS", "sort": "-uploadedDate", "limit": 10,
        })
        print(json.dumps({"builds": [{"id": b["id"], "number": b["attributes"]["version"],
                                     "processingState": b["attributes"]["processingState"]} for b in builds]}))
        return target

    def submit(self, version, build_number):
        target = self.prepare(version)
        deadline = time.monotonic() + 600
        last_state = None
        while True:
            builds = self.collection("/v1/builds", {
                "filter[app]": APP_ID, "filter[preReleaseVersion.version]": version,
                "filter[preReleaseVersion.platform]": "IOS", "filter[version]": build_number,
                "include": "preReleaseVersion", "limit": 200,
            })
            if len(builds) > 1:
                raise RuntimeError("Ambiguous iOS build")
            state = builds[0]["attributes"]["processingState"] if builds else "NOT_VISIBLE"
            if state != last_state:
                print(f"Build {version} ({build_number}): {state}", flush=True)
                last_state = state
            if state in ("VALID", "FAILED", "INVALID"):
                break
            if time.monotonic() >= deadline:
                raise RuntimeError("Apple build processing did not complete within 10 minutes")
            time.sleep(30)
        build = builds[0]
        validate_build(build, build_number)
        pre_release = self.request("GET", f"/v1/builds/{build['id']}/preReleaseVersion")["data"]
        if pre_release["attributes"]["platform"] != "IOS" or pre_release["attributes"]["version"] != version:
            raise RuntimeError("Build belongs to another platform or version")
        self.request("PATCH", f"/v1/appStoreVersions/{target['id']}/relationships/build",
                     data=relation("builds", build["id"]))
        attached = self.request("GET", f"/v1/appStoreVersions/{target['id']}/build")["data"]
        if attached["id"] != build["id"]:
            raise RuntimeError("Wrong build attached")
        submissions = self.collection(f"/v1/apps/{APP_ID}/reviewSubmissions",
                                      {"filter[platform]": "IOS", "limit": 200})
        active = [s for s in submissions if s["attributes"]["state"] != "COMPLETE"]
        if len(active) > 1 or (active and active[0]["attributes"]["state"] != "READY_FOR_REVIEW"):
            raise RuntimeError("Another iOS review submission requires attention")
        submission = active[0] if active else self.request("POST", "/v1/reviewSubmissions", data={"data": {
            "type": "reviewSubmissions", "attributes": {"platform": "IOS"},
            "relationships": {"app": relation("apps", APP_ID)},
        }})["data"]
        sid = submission["id"]
        items = self.collection(f"/v1/reviewSubmissions/{sid}/items")
        if not items:
            self.request("POST", "/v1/reviewSubmissionItems", data={"data": {
                "type": "reviewSubmissionItems", "relationships": {
                    "reviewSubmission": relation("reviewSubmissions", sid),
                    "appStoreVersion": relation("appStoreVersions", target["id"]),
                },
            }})
            items = self.collection(f"/v1/reviewSubmissions/{sid}/items")
        if len(items) != 1 or items[0]["relationships"]["appStoreVersion"]["data"]["id"] != target["id"]:
            raise RuntimeError("Review submission contains unrelated items")
        self.request("PATCH", f"/v1/reviewSubmissions/{sid}", data={"data": {
            "type": "reviewSubmissions", "id": sid, "attributes": {"submitted": True},
        }})
        persisted = self.request("GET", f"/v1/reviewSubmissions/{sid}")["data"]
        state = persisted["attributes"]["state"]
        if state not in ("WAITING_FOR_REVIEW", "IN_REVIEW", "COMPLETE"):
            raise RuntimeError(f"Submission was not queued: {state}")
        version_record = self.request("GET", f"/v1/appStoreVersions/{target['id']}")["data"]
        print(json.dumps({"version": version, "build": build_number, "buildId": build["id"],
                          "versionId": target["id"], "versionState": version_state(version_record),
                          "submissionId": sid, "submissionState": state}))


def relation(kind, identifier):
    return {"data": {"type": kind, "id": identifier}}


def version_state(version):
    return version["attributes"].get("appVersionState") or version["attributes"]["appStoreState"]


def version_number(value):
    return tuple(int(component) for component in value.split("."))


def validate_build(build, build_number):
    attrs = build["attributes"]
    if attrs["version"] != build_number or attrs["processingState"] != "VALID" or attrs["expired"]:
        raise RuntimeError("Build is not valid, unexpired, and equal to the requested build number")
    if attrs.get("usesNonExemptEncryption") is not False:
        raise RuntimeError("Build export compliance is not complete")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["inspect", "prepare", "submit"])
    parser.add_argument("--version")
    parser.add_argument("--build")
    args = parser.parse_args()
    store = AppStore()
    if args.action == "inspect":
        store.inspect()
    else:
        if not args.version or not all(c in "0123456789." for c in args.version):
            parser.error("--version must be a numeric version")
        if args.action == "prepare":
            store.prepare(args.version)
        else:
            if not args.build or not args.build.isdigit():
                parser.error("--build must be the uploaded build number")
            store.submit(args.version, args.build)


if __name__ == "__main__":
    main()
