# Python example

This launcher uses only Python's standard library plus the Ponte Mesh native SDK. A
small `ctypes` wrapper calls the stable C ABI; the download protocol is not
reimplemented in Python.

From the repository root, set `PONTEMESH_SDK_LIBRARY` when the SDK library is not in
`native/`, then run:

```bash
python examples/python/launcher.py
```

The game is installed under `runtime/installations/python/`.
