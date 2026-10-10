"""Find's real Link service, alone and with the real Shelf.

Find indexes a private tree (ARCADE_FIND_HOME/config/settings.json), answers
`find.search` resident and one-shot, opens on `find.show` exactly as Shelf's
"Search in Find" sends it, and its results land in Shelf by reference.
"""
import json
import subprocess
import time


def _tree(s):
    """A small tree and Find settings that index only it."""
    tree = s.root / "find-tree"
    if not tree.exists():
        (tree / "Documents/Reports").mkdir(parents=True)
        (tree / "Pictures").mkdir()
        (tree / "Documents/Reports/report-2024-q3.pdf").write_bytes(b"%PDF-1.4 report")
        (tree / "Pictures/report-cover.png").write_bytes(b"not decoded")
        (tree / "Documents/notes.txt").write_text("unrelated")
        config = s.root / "find/config"
        config.mkdir(parents=True, exist_ok=True)
        (config / "settings.json").write_text(json.dumps({"schema": 1, "roots": [str(tree)]}))
    return tree


def _search(s, query, *args):
    return s.invoke("find", "find.search", "--text", query, *args)


def _ready(s, query, timeout=20):
    deadline = time.monotonic() + timeout
    while True:
        code, reply = _search(s, query)
        if code == 0 and reply["data"]["matched"] and not reply["data"]["indexing"]:
            return reply
        assert time.monotonic() < deadline, reply
        time.sleep(0.2)


def _status(s):
    exe = APPS["arcade.find"]["dir"] / APPS["arcade.find"]["bin"]
    r = subprocess.run([str(exe), "--status"], env=s.env, capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout) if r.returncode == 0 else {}


@check("find")
def find_search_answers_resident_and_one_shot(s):
    tree = _tree(s)
    s.start("arcade.find")
    manifest = json.loads((s.root / "arcade/apps/arcade.find.json").read_text())
    assert manifest["launch"] == {"background": ["--background"], "invoke": ["--arcade-invoke"]}, manifest
    reply = _ready(s, "report")
    types = {o["type"] for o in reply["outputs"]}
    assert "folder/reference" in types and any(t.startswith("file/") for t in types), reply
    paths = [r["path"] for r in reply["data"]["results"]]
    assert str(tree / "Documents/Reports/report-2024-q3.pdf") in paths, paths
    code, reply = _search(s, "report ext:pdf")
    assert code == 0 and [r["name"] for r in reply["data"]["results"]] == ["report-2024-q3.pdf"], reply
    # Stopped Find: launch.invoke answers from the saved index, without a resident.
    s.cli("quit", "find", check=True)
    s.procs.pop("arcade.find", None)
    time.sleep(0.5)
    code, reply = _search(s, "report-cover")
    assert code == 0 and reply["data"]["results"][0]["name"] == "report-cover.png", reply
    assert not s.endpoint("arcade.find").exists(), "one-shot left a resident behind"
    return f"{reply['message']} (one-shot)"


@check("find")
def find_show_opens_on_shelf_search_in_find(s):
    tree = _tree(s)
    if "arcade.find" not in s.procs:
        s.start("arcade.find")
    _ready(s, "report")
    target = tree / "Documents/Reports/report-2024-q3.pdf"
    # What Shelf's "Search in Find" sends for one file item.
    code, reply = s.invoke("find", "find.show", "--input-json",
                           json.dumps({"type": "file/document", "path": str(target), "size": target.stat().st_size}))
    assert code == 0, reply
    s.wait_window("Arcade Find")
    deadline = time.monotonic() + 5
    while _status(s).get("ui", {}).get("selectedPath") != str(target):
        assert time.monotonic() < deadline, _status(s)
        time.sleep(0.1)
    s.screenshot("find-show-from-shelf")
    code, reply = s.invoke("find", "find.show", "--input-json",
                           json.dumps({"type": "folder/reference", "path": str(tree / "Pictures")}))
    assert code == 0, reply
    deadline = time.monotonic() + 5
    while not _status(s).get("ui", {}).get("query", "").startswith("in:"):
        assert time.monotonic() < deadline, _status(s)
        time.sleep(0.1)
    return "Reveal in Find selects the file; a folder scopes the search"


@check("find")
def find_results_land_in_shelf_by_reference(s):
    tree = _tree(s)
    for app in ("arcade.find", "arcade.shelf"):
        if app not in s.procs:
            s.start(app)
    reply = _ready(s, "report")
    args = [value for item in reply["outputs"] for value in ("--input-json", json.dumps(item))]
    code, added = s.invoke("shelf", "shelf.add", *args)
    assert code == 0, added
    expected = len(reply["data"]["results"])
    assert len(added["data"]["added"]) == expected, (reply, added)
    assert {a["path"] for a in added["data"]["added"]} == {r["path"] for r in reply["data"]["results"]}, added
    assert (tree / "Documents/Reports/report-2024-q3.pdf").read_bytes() == b"%PDF-1.4 report"
    return added["message"]
