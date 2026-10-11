# Performance probes

The existing `bench.py --apps ... --compare ...` measurements and regression
checks stay available. All processes use scoped temporary HOME/XDG/ARCADE paths.

For Phase 1, build `arcade-link-cli`, then run:

```sh
python3 benchmarks/bench.py --trayhost --runs 10 --json tray.json
python3 benchmarks/bench.py --hub-scaffold --runs 10 --json hub.json
```

The tray probe measures fake Tools host death to the Rust watcher's `own`
notification, excluding startup. The limit is 1 s, with a 200 ms target.
It does not measure an application's icon painting.

The hub scaffold samples the fake host's Linux process-tree RSS and CPU
over 30 idle seconds. Its mock overlay implements a readiness/frame adapter:
emit `ready`, read `trigger` on stdin, emit `frame` after the first painted
frame. The mock acknowledges the trigger and paints nothing. Replace it with
an instrumented Tools argv using `--overlay-command` as the final argument.
The supplied command inherits only the isolated benchmark environment.

These scaffolds deliberately mark `production_budgets_verified: false`.
Tools' actual closed-webview 40 MiB/zero-tick budget and overlay 80 ms p95
remain for the Tools implementation and its first-frame instrumentation.
Use `--idle-seconds` only to shorten scaffold smoke tests; keep 30 s for the
production idle check.
