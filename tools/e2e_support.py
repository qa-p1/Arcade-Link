"""Owned CLI processes and blocking output reads shared by e2e/bench probes."""
import queue
import subprocess
import threading
import time


class LinesProcess:
    def __init__(self, cli, args, env):
        self.process = subprocess.Popen([str(cli), *args], env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        self.lines = queue.Queue()
        self.errors = []
        def read():
            for line in self.process.stdout:
                self.lines.put((line.strip(), time.perf_counter()))
            self.lines.put((None, time.perf_counter()))
        def errors():
            self.errors.extend(self.process.stderr)
        self.reader = threading.Thread(target=read, daemon=True)
        self.error_reader = threading.Thread(target=errors, daemon=True)
        self.reader.start()
        self.error_reader.start()

    def send(self, command):
        self.process.stdin.write(command + "\n")
        self.process.stdin.flush()

    def expect(self, expected, timeout=10):
        try:
            line, at = self.lines.get(timeout=timeout)
        except queue.Empty:
            raise AssertionError(f"waiting for {expected!r}; stderr: {''.join(self.errors)}") from None
        assert line == expected, f"expected {expected!r}, received {line!r}; stderr: {''.join(self.errors)}"
        return at

    def command(self, command):
        self.send(command)
        return self.expect("ok " + command)

    def silent(self, duration):
        try:
            line, _ = self.lines.get(timeout=duration)
        except queue.Empty:
            return
        raise AssertionError(f"tray flashed/closed during grace: {line!r}")

    def kill(self):
        if self.process.poll() is None:
            self.process.kill()  # kill -9 on POSIX, only a PID this object owns
        self.process.wait(timeout=5)

    def close(self):
        self.kill()
        self.reader.join(timeout=1)
        self.error_reader.join(timeout=1)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            stream.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def tray_handback(cli, env, runs=10, app="find"):
    """Returns per-run spawn-independent SIGKILL → own latencies in ms."""
    samples = []
    for _ in range(runs):
        with LinesProcess(cli, ["tray", "host"], env) as host:
            host.expect("ready")
            with LinesProcess(cli, ["tray", app], env) as watcher:
                watcher.expect("hosted")
                started = time.perf_counter()
                host.kill()
                at = watcher.expect("own")
                samples.append(round((at - started) * 1000, 3))
    return samples
