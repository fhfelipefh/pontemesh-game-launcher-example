# JavaScript example

This Node.js launcher uses [Koffi](https://koffi.dev/) to call the Ponte Mesh native
C ABI. Koffi is only the language bridge; all authorization, source selection,
fragment validation, and fallback remain inside the official SDK.

From the repository root:

```bash
cd examples/javascript
npm install
npm start
```

Set `PONTEMESH_SDK_LIBRARY` when the library is not in `native/`. The game is
installed under `runtime/installations/javascript/`.
