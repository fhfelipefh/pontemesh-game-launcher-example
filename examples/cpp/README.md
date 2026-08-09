# C++ example

This launcher loads the Ponte Mesh C ABI dynamically and keeps the native handle in
an RAII C++ class. It uses CMake and a pinned nlohmann/json release for the shared
release descriptor.

Configure `PONTEMESH_SDK_INCLUDE_DIR` when `pontemesh_sdk.h` is not available from a
sibling SDK checkout or `native/include/`:

```bash
cmake -S examples/cpp -B examples/cpp/build
cmake --build examples/cpp/build --config Release
```

Set `PONTEMESH_SDK_LIBRARY` when the dynamic library is not in `native/`, then run
the generated `pontemesh_game_launcher_cpp` from the repository root. The game is
installed under `runtime/installations/cpp/`.
