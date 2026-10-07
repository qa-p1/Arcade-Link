"""Look server and consumer checks; loaded by the isolated ecosystem runner."""
import json
import hashlib
import os
import socket
import signal
import sqlite3
import uuid
import struct
import subprocess
import time
import zlib


def png(path, width=12, height=7):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = (b"\0" + b"\xc8\x28\x28" * width) * height
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    return path


def running(s):
    s.env["ALOOK_DEBUG"] = "1"
    s.env["ALOOK_E2E_MAP_EARLY"] = "1"
    s.env["ALOOK_E2E_CONTROL"] = str(s.root / "look-ui.sock")
    if "arcade.look" not in s.procs or s.procs["arcade.look"].poll() is not None:
        (s.root / "look-ui.sock").unlink(missing_ok=True)
        s.start("arcade.look")


def wait_log(s, text):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if text in s.log("arcade.look"):
            return
        time.sleep(.05)
    raise AssertionError(s.log("arcade.look"))


@check("look")
def inspect_resident_image_and_folder(s):
    running(s)
    image = png(s.root / "look-inspect.png")
    code, result = s.invoke("look", "look.inspect", "--file", str(image))
    assert code == 0, result
    info = result["outputs"][0]["data"]
    assert (info["width"], info["height"], info["type"]) == (12, 7, "file/image"), info
    code, result = s.invoke("look", "look.inspect", "--input-json", json.dumps({"type": "folder/reference", "path": str(s.root)}))
    assert code == 0 and result["outputs"][0]["data"]["kind"] == "folder", result
    return json.dumps(info)


@check("look")
def inspect_oneshot_without_ui_or_presence(s):
    image = png(s.root / "look-oneshot.png", 3, 2)
    spec = APPS["arcade.look"]
    for path, kind in [(image, "file/image"), (s.root, "folder/reference")]:
        req = {"v": 1, "id": 17, "method": "invoke", "params": {
            "action": "look.inspect", "inputs": [{"type": kind, "path": str(path)}],
            "context": {"source": "e2e", "interactive": False, "reason": "test"}}}
        r = subprocess.run([str(spec["dir"] / spec["bin"]), "--arcade-invoke"], input=json.dumps(req) + "\n",
                           env=s.env, capture_output=True, text=True, timeout=20)
        assert r.returncode == 0, r.stdout + r.stderr
        reply = json.loads(r.stdout.strip().splitlines()[-1])
        info = reply["result"]["outputs"][0]["data"]
        assert info["type"] == kind, info
        if kind == "file/image":
            assert (info["width"], info["height"]) == (3, 2), info
        assert "setup" not in r.stderr and "creating window" not in r.stderr, r.stderr
    return "one-shot image 3x2 + folder; no Tauri setup/window"


@check("look")
def preview_single_batch_and_file_url(s):
    running(s)
    for name, args in [("look-single.png", "file"), ("look-batch.png", "batch"), ("look url.png", "url")]:
        image = png(s.root / name)
        inputs = ["--url", image.as_uri()] if args == "url" else ["--file", str(image)]
        if args == "batch":
            inputs += ["--file", str(png(s.root / "look-batch-second.png"))]
        code, result = s.invoke("look", "look.preview", *inputs)
        assert code == 0 and result["message"] == ("Previewing 2 files" if args == "batch" else f"Previewing {name}"), result
        wait_log(s, f'open Some("{image}") from Local')
    return "single, batch, encoded file:// URL reached app::open (rendering not asserted)"


@check("look")
def unsupported_url_and_linux_selection(s):
    running(s)
    code, result = s.invoke("look", "look.preview", "--url", "https://example.com")
    assert code != 0 and "unsupported_input" in result, result
    actions = json.loads(s.cli("describe", "look", "--json", check=True).stdout)
    assert "look.preview_selection" not in [a["id"] for a in actions], actions
    return "non-file URL rejected; preview_selection absent on Linux"


@check("look")
def look_consumer_boundaries(s):
    deps = APPS["arcade.look"]["dir"] / "src-tauri/target/link-tests/debug/deps"
    binaries = [p for p in deps.glob("link_consumer-*") if p.is_file() and os.access(p, os.X_OK)]
    assert binaries, "build Look's consumer tests first: cargo test --test link_consumer --no-run"
    binary = max(binaries, key=lambda p: p.stat().st_mtime)
    r = subprocess.run([str(binary), "--ignored", "--nocapture", "--test-threads=1"], env=s.env,
                       capture_output=True, text=True, timeout=90)
    assert r.returncode == 0, r.stdout + r.stderr
    assert "0 failed" in r.stdout and "8 passed" in r.stdout, r.stdout
    return r.stdout.strip()


@check("look")
def master_switch_stops_presence_and_restores_it(s):
    running(s)
    config_path = s.root / "config/arcade-look/config.json"
    original = config_path.read_text()
    config = json.loads(original)
    image = png(s.root / "look-switch.png")
    spec = APPS["arcade.look"]

    def reopen():
        r = subprocess.run([str(spec["dir"] / spec["bin"]), str(image)], env=s.env,
                           capture_output=True, text=True, timeout=20)
        assert r.returncode == 0, r.stdout + r.stderr

    try:
        config["linkEnabled"] = False
        config["linkDisabledPeers"] = ["arcade.box"]
        config_path.write_text(json.dumps(config))
        reopen()  # Existing standalone single-instance channel reloads config.
        deadline = time.monotonic() + 10
        while s.endpoint("arcade.look").exists() and time.monotonic() < deadline:
            time.sleep(.05)
        assert not s.endpoint("arcade.look").exists(), s.log("arcade.look")
        manifest = json.loads((s.root / "arcade/apps/arcade.look.json").read_text())
        assert manifest["settings"]["linkEnabled"] is False and manifest["actions"] == [], manifest
        assert s.procs["arcade.look"].poll() is None, s.log("arcade.look")
    finally:
        config_path.write_text(original)
        reopen()
        s.wait_running("arcade.look")
    return "master off: endpoint removed + no actions; standalone preview still delivered; on: presence restored"


def ui(s, expression, timeout=10):
    """Evaluate in the real native webview through the test-only feature."""
    token = uuid.uuid4().hex
    script = """(async () => {
      let response;
      try { response = { id: TOKEN, value: await (EXPRESSION) }; }
      catch (e) { response = { id: TOKEN, error: String(e) }; }
      await window.__TAURI_INTERNALS__.invoke('log', { level: 'e2e', message: JSON.stringify(response) });
    })();""".replace('TOKEN', json.dumps(token)).replace('EXPRESSION', expression)
    deadline = time.monotonic() + timeout
    control = s.root / "look-ui.sock"
    log = s.root / "arcade.look.log"
    while "frontend ready" not in log.read_text(errors="replace"):
        assert time.monotonic() < deadline, s.log("arcade.look")
        time.sleep(.05)
    while not control.exists():
        assert time.monotonic() < deadline, "build Look with --features e2e; native control socket missing"
        time.sleep(.05)
    with socket.socket(socket.AF_UNIX) as sock:
        sock.connect(str(control))
        sock.sendall((json.dumps(script) + "\n").encode())
    while time.monotonic() < deadline:
        for line in log.read_text(errors="replace").splitlines():
            if 'ui e2e: ' in line and token in line:
                response = json.loads(line.split('ui e2e: ', 1)[1])
                assert 'error' not in response, response
                return response.get('value')
        time.sleep(.02)
    raise AssertionError(f"native webview did not reply to {expression}: {s.log('arcade.look')}")


def shot(s, name, window):
    ui(s, "(async()=>{await Promise.all(document.getAnimations().filter(a=>a.effect?.getComputedTiming().iterations !== Infinity).map(a=>a.finished.catch(()=>{}))); await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))); return true;})()")
    return s.screenshot(name, window)


def wait_ui(s, expression, timeout=15):
    deadline = time.monotonic() + timeout
    value = None
    while time.monotonic() < deadline:
        value = ui(s, expression)
        if value:
            return value
        time.sleep(.05)
    raise AssertionError(f"UI condition failed: {expression}; value={value}; {s.log('arcade.look')}")


@check("look")
def native_ui_boots_without_peers(s):
    running(s)
    image = png(s.root / "look-no-peers.png", 320, 200)
    code, result = s.invoke("look", "look.preview", "--file", str(image))
    assert code == 0, result
    wait_ui(s, "document.querySelector('.title-name')?.textContent === 'look-no-peers.png' && !!document.querySelector('.viewer-host img')")
    assert ui(s, "document.querySelector('.arcade-actions') === null")
    win = s.wait_window('look-no-peers.png')
    shot(s, 'look-strip-no-peers', win)
    return "mapped native webview renders image; no actions strip with no peers"


def preview(s, path):
    code, result = s.invoke("look", "look.preview", "--file", str(path))
    assert code == 0, result
    wait_ui(s, f"document.querySelector('.title-name')?.textContent === {json.dumps(path.name)} && !!document.querySelector('.viewer-host:not(.pending)')")
    return s.wait_window(path.name)


def offers(s):
    return ui(s, "[...document.querySelectorAll('.arcade-offer')].map(b => ({title:b.getAttribute('aria-label'), disabled:b.disabled, detail:b.title}))")


def click_offer(s, title):
    ui(s, f"(() => {{ const b=[...document.querySelectorAll('.arcade-offer')].find(b=>b.getAttribute('aria-label') === {json.dumps(title)}); if (!b || b.disabled) throw new Error('Action missing or disabled'); b.click(); return true; }})()")


def open_strip(s, window):
    s.xdotool("windowfocus", window)
    if ui(s, "document.querySelector('.arcade-menu')?.hidden ?? true"):
        s.xdotool("key", "a")
    wait_ui(s, "document.querySelector('.arcade-menu')?.hidden === false")


def look_mock(s, peer, actions, shortcuts=()):
    s.kill(peer)
    (s.root / "arcade/run" / f"{peer}.endpoint").unlink(missing_ok=True)
    fixture = s.root / f"{peer}-look-mock.json"
    fixture.write_text(json.dumps({"id": peer, "actions": actions, "shortcuts": list(shortcuts)}))
    env = dict(s.env, ARCADE_MOCK_LOG=str(s.root / f"{peer}-look-calls.jsonl"))
    p = subprocess.Popen([str(CLI), "mock", "--as", peer, "--actions", str(fixture)], env=env,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    s.procs[peer] = p
    s.wait_running(peer)
    return p


def remove_peer(s, peer):
    s.kill(peer)
    (s.root / "arcade/apps" / f"{peer}.json").unlink(missing_ok=True)
    (s.root / "arcade/run" / f"{peer}.endpoint").unlink(missing_ok=True)


def settings(s):
    spec = APPS["arcade.look"]
    r = subprocess.run([str(spec["dir"] / spec["bin"]), "--settings"], env=s.env,
                       capture_output=True, text=True, timeout=20)
    assert r.returncode == 0, r.stderr
    wait_ui(s, "document.querySelectorAll('.arcade-peer').length === 4")
    ui(s, "document.querySelector('#connected-apps').scrollIntoView({block:'start'})")
    return s.wait_window('Settings.*Arcade Look')


@check("look")
def native_connected_settings_and_shortcut_warning(s):
    running(s)
    image = png(s.root / "look-settings.png", 320, 200)
    preview(s, image)
    window = settings(s)
    assert ui(s, "document.querySelectorAll('.arcade-peer button').length === 4")
    assert ui(s, "document.querySelectorAll('.arcade-peer input').length === 0")
    ui(s, "document.querySelector('.arcade-diagnostics').open = true")
    shot(s, 'look-connected-apps-missing', window)
    action = {"id":"box:arcade.image.convert#webp", "title":"Convert to WebP", "preset":"webp",
              "accepts":["file/image"], "featuredFor":["file/image"]}
    try:
        look_mock(s, "arcade.box", [action], [{"id":"island","accelerator":"Ctrl+Alt+Space"}])
        wait_ui(s, "document.querySelector('.arcade-peer small')?.textContent?.startsWith('Running')")
        assert ui(s, "document.querySelectorAll('.arcade-peer input').length === 1 && document.querySelector('.arcade-peer .setting-row span')?.textContent === 'Use with Arcade Look'")
        assert ui(s, "Math.abs(document.querySelector('.arcade-master input').getBoundingClientRect().right - document.querySelector('.arcade-peer input').getBoundingClientRect().right) < 2")
        ui(s, "document.querySelector('.arcade-shortcut input').scrollIntoView({block:'center'})")
        ui(s, "document.querySelector('.arcade-shortcut input').focus()")
        s.xdotool("windowfocus", window)
        s.xdotool("key", "ctrl+alt+space")
        wait_ui(s, "document.querySelector('.arcade-shortcut [role=status]')?.textContent === 'Used by Arcade Box'")
        shot(s, 'look-shortcut-clash', window)
        ui(s, "document.querySelector('.arcade-peer input').click()")
        wait_ui(s, "document.querySelector('.arcade-peer input')?.checked === false && !document.querySelector('.arcade-shortcut input')?.disabled")
        ui(s, "document.querySelector('#connected-apps').scrollIntoView({block:'start'})")
        shot(s, 'look-peer-toggle-off', window)
        win = preview(s, image)
        wait_ui(s, "document.querySelector('.arcade-actions') === null")
        shot(s, 'look-strip-peer-off', win)
        window = settings(s)
        ui(s, "document.querySelector('.arcade-peer input').click()")
        wait_ui(s, "document.querySelector('.arcade-peer input')?.checked === true")
        ui(s, "document.querySelector('input[aria-label=\"Connect with other Arcade apps\"]').click()")
        wait_ui(s, "[...document.querySelectorAll('.arcade-peer input')].every(e=>e.disabled)")
        assert not s.endpoint('arcade.look').exists()
        shot(s, 'look-connected-apps-master-off', window)
        ui(s, "document.querySelector('input[aria-label=\"Connect with other Arcade apps\"]').click()")
        s.wait_running('arcade.look')
        wait_ui(s, "document.querySelector('.arcade-peer input')?.disabled === false")
        ui(s, "document.querySelector('.arcade-diagnostics').open = true")
        shot(s, 'look-connected-apps', window)
        ui(s, "window.dispatchEvent(new KeyboardEvent('keydown',{key:'?',bubbles:true}))")
        wait_ui(s, "document.querySelector('.help')?.textContent?.includes('Shift+A')")
        shot(s, 'look-help', window)
        ui(s, "document.querySelector('.modal-backdrop').click()")
    finally:
        remove_peer(s, "arcade.box")
        ui(s, "window.__TAURI_INTERNALS__.invoke('set_config',{patch:{linkEnabled:true,linkDisabledPeers:[]}})")
    return "Connected apps rows/labels/diagnostics; live peer; recorder clash; peer/master toggles; help keys"


@check("look")
def native_strip_unavailable_cancel_crash_and_cached_pipelines(s):
    running(s)
    image = png(s.root / "look-boundaries.png", 320, 200)
    win = preview(s, image)
    action = {"id":"box:arcade.image.convert#webp", "title":"Convert to WebP", "preset":"webp",
              "accepts":["file/image"], "featuredFor":["file/image"]}
    try:
        look_mock(s, "arcade.box", [{**action, "available":False, "reason":"Converter disabled"}])
        time.sleep(.25)
        assert not offers(s), offers(s)
        assert ui(s, "document.querySelector('.arcade-actions') === null")
        look_mock(s, "arcade.box", [{**action, "mock":{"steps":100,"stepMs":30}}])
        wait_ui(s, "document.querySelectorAll('.arcade-offer').length === 1")
        open_strip(s, win)
        click_offer(s, 'Convert to WebP')
        wait_ui(s, "document.querySelector('.arcade-job')?.hidden === false")
        heartbeat = ui(s, "new Promise(resolve=>{const start=performance.now();requestAnimationFrame(()=>resolve(performance.now()-start));})")
        assert heartbeat < 200, heartbeat
        shot(s, 'look-progress-chip', win)
        ui(s, "document.querySelector('button[aria-label=\"Cancel action\"]').click()")
        wait_ui(s, "document.querySelector('.toast.on')?.textContent === 'Cancelled.'")
        wait_ui(s, "document.querySelector('.arcade-job')?.hidden === true")
        look_mock(s, "arcade.box", [{**action, "mock":{"steps":100,"stepMs":30,"crashAfterMs":120}}])
        wait_ui(s, "document.querySelectorAll('.arcade-offer').length === 1")
        click_offer(s, 'Convert to WebP')
        wait_ui(s, "document.querySelector('.toast.on')?.textContent === \"Arcade Box isn't running.\"")
        pipelines = [
            {"id":"web-image","name":"Web image","version":1,"accepts":["file/image"],"produces":["file/image"],"effects":["writes-files"],"interactive":False},
            {"id":"capture","name":"Capture first","version":1,"accepts":["file/image"],"produces":[],"effects":[],"interactive":True},
            {"id":"video","name":"Video only","version":1,"accepts":["file/video"],"produces":[],"effects":[],"interactive":False},
        ]
        look_mock(s, "arcade.box", [
            {"id":"box.pipelines","title":"Pipelines","mock":{"result":{"outputs":[{"type":"structured/pipelines","data":pipelines}]}}},
            {"id":"box.pipeline.run","title":"Run pipeline","accepts":["file/*"]},
        ])
        wait_ui(s, "document.querySelector('.arcade-offer')?.getAttribute('aria-label') === '▶ Web image'")
        calls = s.root / 'arcade.box-look-calls.jsonl'
        before = calls.read_text()
        open_strip(s, win)
        wait_ui(s, "!document.querySelector('.toast.on')")
        shot(s, 'look-saved-pipelines', win)
        assert calls.read_text() == before, 'opening the strip queried Box'
        assert [o['title'] for o in offers(s)] == ['▶ Web image']
        click_offer(s, '▶ Web image')
        wait_ui(s, "document.querySelector('.arcade-job')?.hidden === true")
        call = json.loads(calls.read_text().splitlines()[-1])
        assert call['options']['pipeline'] == 'web-image', call
    finally:
        remove_peer(s, 'arcade.box')
    return f"unavailable hidden; cancel/crash standard errors; UI frame during IPC={heartbeat:.1f}ms; cached pipeline filtered and invoked"


def pdf(path):
    objects = [
        b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
        b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
    ]
    stream = b'BT /F1 22 Tf 60 700 Td (Arcade Look PDF handoff) Tj ET\n' + b'% compressible fixture\n' * 500
    objects.append(b'<< /Length ' + str(len(stream)).encode() + b' >>\nstream\n' + stream + b'endstream')
    data = b'%PDF-1.4\n'
    offsets = [0]
    for n, obj in enumerate(objects, 1):
        offsets.append(len(data))
        data += f'{n} 0 obj\n'.encode() + obj + b'\nendobj\n'
    xref = len(data)
    data += f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode()
    data += b''.join(f'{offset:010} 00000 n \n'.encode() for offset in offsets[1:])
    data += f'trailer\n<< /Root 1 0 R /Size {len(offsets)} >>\nstartxref\n{xref}\n%%EOF\n'.encode()
    path.write_bytes(data)
    return path


def seed_real_clipboard(s):
    driver = APPS['arcade.clipboard']['dir'] / 'target/debug/arcade_test_driver'
    assert driver.exists(), 'real Clipboard core test driver is missing'
    requests = [
        {'op':'initialize','data_dir':s.env['ARCADE_DATA_DIR'],'device_name':'Look e2e','link':{'enabled':False}},
        {'op':'create_mesh','device_name':'Look e2e'}, {'op':'shutdown'},
    ]
    result = subprocess.run([str(driver)], input=''.join(json.dumps(r)+'\n' for r in requests), env=s.env,
                            capture_output=True, text=True, timeout=30)
    assert result.returncode == 0, result.stderr
    replies = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(replies) == len(requests) and all('ok' in reply for reply in replies), replies
    s.start('arcade.clipboard')


@check('look')
def real_box_image_video_pdf_flagship_flows(s):
    running(s)
    s.start('arcade.box')
    # Seed a real saved file pipeline in Box's isolated desktop database.
    s.kill('arcade.box', signal.SIGTERM)
    pipeline = {'id':'look-web-image','name':'Web image','version':1,
                'nodes':[{'id':'convert','toolId':'arcade.image.convert','inputs':[{'kind':'external','index':0}],
                          'options':{'format':'webp','quality':82}}], 'outputNodes':['convert']}
    from pathlib import Path
    db = Path(s.env['XDG_DATA_HOME']) / 'dev.arcadebox.app/arcade.sqlite3'
    with sqlite3.connect(db) as connection:
        connection.execute('INSERT OR REPLACE INTO pipelines(id,version,definition_json) VALUES(?,?,?)',
                           (pipeline['id'],1,json.dumps(pipeline)))
    s.start('arcade.box')
    seed_real_clipboard(s)
    s.start('arcade.lens')
    s.start('arcade.wheel')
    window = settings(s)
    wait_ui(s, "document.querySelectorAll('.arcade-peer input').length === 4")
    ui(s, "document.querySelector('.arcade-diagnostics').open = true")
    shot(s, 'look-connected-apps-real', window)
    ui(s, "(() => { const input=document.querySelector('input[aria-label=\"Play video and audio automatically\"]'); if(input.checked) input.click(); return true; })()")
    wait_ui(s, "!document.querySelector('input[aria-label=\"Play video and audio automatically\"]')?.checked && !document.querySelector('input[aria-label=\"Play video and audio automatically\"]')?.disabled")
    image = png(s.root / 'look-real-image.png', 480, 300)
    win = preview(s, image)
    wait_ui(s, "[...document.querySelectorAll('.arcade-offer')].some(b=>b.getAttribute('aria-label') === '▶ Web image')", timeout=30)
    labels = {offer['title'] for offer in offers(s)}
    assert {'Convert to WebP','More in Arcade Box…','Send to my devices ↗','Analyze with Lens','Add to Wheel'} <= labels, labels
    assert ui(s, "[...document.querySelectorAll('.arcade-offer')].filter(b=>/^(Convert|More in|▶|Add to)/.test(b.getAttribute('aria-label'))).every(b=>!b.querySelector('small'))")
    assert ui(s, "[...document.querySelectorAll('.arcade-offer')].find(b=>b.getAttribute('aria-label') === 'Send to my devices ↗')?.querySelector('small')?.textContent?.includes('look-real-image.png') && ![...document.querySelectorAll('.arcade-offer')].find(b=>b.getAttribute('aria-label') === 'Send to my devices ↗')?.querySelector('small')?.textContent?.includes('/tmp/')")
    open_strip(s, win)
    shot(s, 'look-strip-image', win)
    click_offer(s, '▶ Web image')
    wait_ui(s, "document.querySelector('.title-name')?.textContent?.endsWith('.webp') && document.querySelector('.arcade-job')?.hidden === true", timeout=60)
    path = ui(s, "document.querySelector('.title-name').title")
    assert Path(path).read_bytes()[8:12] == b'WEBP', path
    win = s.wait_window('.*webp.*Arcade Look')
    shot(s, 'look-real-pipeline-result', win)
    wait_ui(s, "[...document.querySelectorAll('.arcade-offer')].some(b=>b.getAttribute('aria-label') === 'Send to my devices ↗' && !b.disabled)")
    click_offer(s, 'Send to my devices ↗')
    wait_ui(s, "document.querySelector('.toast.on')?.textContent === 'Sent to your devices'")
    video = s.root / 'look-sharing.mp4'
    result = subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-y','-threads','3','-filter_threads','3',
                             '-f','lavfi','-i','testsrc2=size=1280x720:rate=24','-t','8','-c:v','libx264','-threads','3',
                             '-preset','ultrafast','-crf','20',str(video)], env=s.env, capture_output=True, text=True, timeout=60)
    assert result.returncode == 0, result.stderr
    # Valid MP4 free box makes the source too large for Clipboard without a long fixture encode.
    with video.open('ab') as output:
        output.write(struct.pack('>I4s',17*1024*1024+8,b'free') + b'\0'*(17*1024*1024))
    original = hashlib.sha256(video.read_bytes()).hexdigest()
    win = preview(s, video)
    wait_ui(s, "[...document.querySelectorAll('.arcade-offer')].some(b=>b.getAttribute('aria-label') === 'Compress for sharing')", timeout=30)
    send = next(o for o in offers(s) if o['title'] == 'Send to my devices ↗')
    assert send['disabled'] and send['detail'] == 'Too large to send to your devices (limit 16 MB).', send
    open_strip(s, win)
    shot(s, 'look-strip-video', win)
    click_offer(s, 'Compress for sharing')
    wait_ui(s, "document.querySelector('.arcade-job')?.hidden === false")
    shot(s, 'look-flow3-progress', win)
    wait_ui(s, "document.querySelector('.title-name')?.textContent !== 'look-sharing.mp4' && document.querySelector('.title-name')?.textContent?.endsWith('.mp4') && document.querySelector('.arcade-job')?.hidden === true", timeout=120)
    output = Path(ui(s, "document.querySelector('.title-name').title"))
    assert output != video and output.stat().st_size <= 16*1024*1024, output
    assert hashlib.sha256(video.read_bytes()).hexdigest() == original
    win = s.wait_window('.*mp4.*Arcade Look')
    wait_ui(s, "[...document.querySelectorAll('.arcade-offer')].some(b=>b.getAttribute('aria-label') === 'Send to my devices ↗' && !b.disabled)")
    open_strip(s, win)
    shot(s, 'look-flow3-result-send-enabled', win)
    doc = pdf(s.root / 'look-colleague.pdf')
    win = preview(s, doc)
    wait_ui(s, "document.querySelector('.pdf-page canvas')?.width > 0 && [...document.querySelectorAll('.arcade-offer')].some(b=>b.getAttribute('aria-label') === 'Compress PDF')", timeout=30)
    open_strip(s, win)
    shot(s, 'look-strip-pdf', win)
    click_offer(s, 'Compress PDF')
    wait_ui(s, "document.querySelector('.arcade-job')?.hidden === false")
    shot(s, 'look-flow10-progress', win)
    wait_ui(s, "document.querySelector('.title-name')?.textContent !== 'look-colleague.pdf' && document.querySelector('.title-name')?.textContent?.endsWith('.pdf') && document.querySelector('.arcade-job')?.hidden === true", timeout=120)
    output = Path(ui(s, "document.querySelector('.title-name').title"))
    assert output != doc and output.read_bytes().startswith(b'%PDF-'), output
    wait_ui(s, "document.querySelector('.pdf-page canvas')?.width > 0", timeout=30)
    win = s.wait_window('.*pdf.*Arcade Look')
    shot(s, 'look-flow10-result', win)
    wait_ui(s, "[...document.querySelectorAll('.arcade-offer')].some(b=>b.getAttribute('aria-label') === 'Analyze with Lens')")
    click_offer(s, 'Analyze with Lens')
    lens = s.wait_window('Arcade Lens')
    s.xdotool('windowfocus', lens)
    shot(s, 'look-pdf-page-in-lens', lens)
    s.xdotool('key','Escape')
    wait_ui(s, "document.querySelector('.arcade-job')?.hidden === true", timeout=30)
    assert not list((s.root/'arcade/handoff').glob('*/page.png')), 'PDF handoff was not removed'
    click_offer(s, 'Add to Wheel')
    # Wheel may need launching first (Link launch-on-demand), then shows its
    # Settings with the file pre-filled and waits for the user. Cancelling from
    # Look's chip must end that pending request in Wheel.
    wheel = s.wait_window('Arcade Wheel.*Settings', timeout=20)
    s.xdotool('windowraise', wheel)
    time.sleep(0.4)
    shot(s, 'look-file-in-wheel', wheel)
    wait_ui(s, "document.querySelector('.arcade-job')?.hidden === false")
    ui(s, "document.querySelector('button[aria-label=\"Cancel action\"]').click()")
    wait_ui(s, "document.querySelector('.arcade-job')?.hidden === true", timeout=30)
    return 'real Box saved pipeline + WebP result; real Clipboard send; flow 3 progress/result/send enabled; flow 10 PDF result; real Lens PDF handoff and Wheel file editor'


@check("look")
def startup_mode_and_linux_resolve_only(s):
    running(s)
    status = json.loads(s.cli("status", "look", "--json", check=True).stdout)
    assert status["status"]["mode"] == "background", status
    before = s.log("arcade.look").count("open Some(")
    code, result = s.invoke("look", "look.preview_selection", "--option", "resolveOnly=true")
    assert code != 0 and "unavailable" in result, result
    assert s.log("arcade.look").count("open Some(") == before, "resolveOnly opened a window"
    manifest = json.loads(s.cli("describe", "look", "--json", check=True).stdout)
    assert not any(a["id"] == "look.preview_selection" for a in manifest), manifest
    s.kill('arcade.look')
    s.endpoint('arcade.look').unlink(missing_ok=True)
    (s.root / 'look-ui.sock').unlink(missing_ok=True)
    spec = APPS['arcade.look']
    saved = spec['args']
    try:
        spec['args'] = ['--settings']
        s.start('arcade.look')
        status = json.loads(s.cli('status', 'look', '--json', check=True).stdout)
        assert status['status']['mode'] == 'foreground', status
        result = subprocess.run([str(spec['dir'] / spec['bin']), '--background'], env=s.env,
                                capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, result.stderr
        status = json.loads(s.cli('status', 'look', '--json', check=True).stdout)
        assert status['status']['mode'] == 'foreground', status
    finally:
        spec['args'] = saved
    return "background/foreground startup mode survives later requests; Linux resolveOnly unavailable, no UI, action hidden"
