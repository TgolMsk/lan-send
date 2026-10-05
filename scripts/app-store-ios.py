#!/usr/bin/env python3
"""Manage this app's iOS release using ASC credentials kept on the CI runner."""
import argparse
import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request

APP_ID = "6809459213"
BUNDLE_ID = "com.wangsheng.lansend"
BASE = "https://api.appstoreconnect.apple.com"


class AppStore:
    def request(self, method, path, *, query=None, data=None):
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["inspect"])
    parser.parse_args()
    AppStore().inspect()


if __name__ == "__main__":
    main()
