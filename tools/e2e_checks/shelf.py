"""Shelf's real resident persistence, input encoding and picker cancellation.

These checks send Find's selection encoding directly; the `find` group runs
the real Find against the real Shelf.
"""
import json
import time


@check("shelf")
def shelf_resident_adds_mixed_references_and_reports_skips(s):
    s.start("arcade.shelf")
    manifest = json.loads((s.root / "arcade/apps/arcade.shelf.json").read_text())
    assert manifest["launch"]["background"] == ["--background"], manifest
    assert "invoke" not in manifest["launch"], manifest
    first = s.root / "first.txt"
    second = s.root / "second.png"
    folder = s.root / "folder"
    first.write_text("Original content")
    second.write_bytes(b"Referenced, never decoded during add")
    folder.mkdir()
    code, reply = s.invoke("shelf", "shelf.add", "--file", str(first))
    assert code == 0 and len(reply["data"]["added"]) == 1, reply
    assert reply["data"]["shelf"]["name"] and reply["message"], reply
    mixed = [{"type": "file/any[]", "paths": [str(first), str(second), str(s.root / "missing")]},
             {"type": "folder/reference", "path": str(folder)}]
    args = [value for item in mixed for value in ("--input-json", json.dumps(item))]
    code, reply = s.invoke("shelf", "shelf.add", *args)
    assert code == 0 and len(reply["data"]["added"]) == 2, reply
    assert any(item["reason"] == "duplicate" for item in reply["data"]["skipped"]), reply
    assert any(item["path"] == str(s.root / "missing") for item in reply["data"]["skipped"]), reply
    assert first.read_text() == "Original content"
    assert second.exists() and folder.exists()
    code, reply = s.invoke("shelf", "shelf.add", "--input-json",
                           json.dumps({"type": "folder/reference", "path": str(s.root / "missing")}))
    assert code != 0, reply
    return "Real Shelf; Find's selection encoding, duplicates and missing references"


@check("shelf")
def shelf_relaunch_uses_background_resident_and_recovers_references(s):
    if "arcade.shelf" not in s.procs:
        s.start("arcade.shelf")
    original = s.root / "relaunch.txt"
    original.write_text("Do not copy or delete")
    code, reply = s.invoke("shelf", "shelf.add", "--file", str(original))
    assert code == 0, reply
    s.kill("arcade.shelf")
    started = time.monotonic()
    try:
        code, reply = s.invoke("shelf", "shelf.add", "--file", str(original), timeout=5)
        assert code == 0, reply
        assert s.endpoint("arcade.shelf").exists(), reply
        assert time.monotonic() - started < 3.5
        assert len(reply["data"]["added"]) == 0, reply
        assert reply["data"]["skipped"][0]["reason"] == "duplicate", reply
        assert original.read_text() == "Do not copy or delete"
    finally:
        # invoke_action owns this detached launch; Session's Popen table does
        # not. Shut down precisely this private-profile Link endpoint.
        s.cli("quit", "shelf", "--force")
        # Quitting is asynchronous: a Shelf started before this one has
        # released its endpoint and instance lock would hand off to it and exit.
        deadline = time.monotonic() + 5
        while s.endpoint("arcade.shelf").exists() and time.monotonic() < deadline:
            time.sleep(0.05)
    return "Stopped Shelf launched through launch.background; SQLite state recovered"


@check("shelf")
def shelf_picker_cancellation_returns_no_collection_contents(s):
    s.start("arcade.shelf")
    pending = s.invoke("shelf", "shelf.pick", background=True)
    window = s.wait_window("Arcade Shelf")
    s.xdotool("windowfocus", window, "key", "Escape")
    out, err = pending.communicate(timeout=10)
    assert pending.returncode != 0, out + err
    assert "Cancelled" in out + err, out + err
    assert '"outputs"' not in out, out
    return "Interactive cancellation; no shelf contents returned"
