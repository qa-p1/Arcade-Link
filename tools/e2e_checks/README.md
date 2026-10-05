# e2e check modules

Each `*.py` here is loaded by `tools/e2e.py`. Names injected into the module:
`check(group)` (decorator), `Session`, `APPS`, `CLI`. A check takes the
`Session` and returns an optional detail string; raise `AssertionError` to fail.

```python
@check("look")
def inspect_reports_dimensions(s):
    s.start("arcade.look")
    r = s.cli("invoke", "look", "look.inspect", "--file", str(png), "--json", check=True)
    assert '"width": 2' in r.stdout, r.stdout
```

Run one group: `python3 tools/e2e.py --only look`.
