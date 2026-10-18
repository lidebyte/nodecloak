"""Build the deterministic source ZIP for Mozilla AMO signing; no credentials."""
import argparse
import json
import pathlib
import zipfile


def build(destination):
    root = pathlib.Path(__file__).resolve().parents[1]
    source = root / "extensions" / "firefox"
    manifest = json.loads((source / "manifest.json").read_text(encoding="utf-8"))
    destination = pathlib.Path(destination).resolve()
    destination.mkdir(parents=True, exist_ok=True)
    archive = destination / ("nodecloak-firefox-" + manifest["version"] + "-unsigned.zip")
    files = {
        "manifest.json": (source / "manifest.json").read_bytes(),
        "background.js": (source / "background.js").read_bytes(),
        "fingerprint.js": (root / "src-tauri" / "src" / "fingerprint.js").read_bytes(),
        "LICENSE": (root / "LICENSE").read_bytes(),
    }
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            output.writestr(info, content)
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", default=".qa/firefox-extension")
    print(build(parser.parse_args().output))
