# noVNC for the web remote

`novnc.min.js.gz` is [noVNC](https://github.com/novnc/noVNC) 1.7.0 (`@novnc/novnc` on npm, MPL-2.0, `LICENSE.txt`
here): `core/rfb.js` and everything it imports, as one minified ES module, gzip. Nothing in it is changed.
`ui/src/vnc.rs` puts it into the program (`include_bytes!`) and serves it as `/novnc.js` for the page at `/vnc`;
the server side is `ui/src/vnc.rs`.

| | bytes |
|---|---|
| npm package, unpacked | 634 905 |
| one minified module | 188 687 |
| gzip, as it lies in the program | 56 820 |

## Making it again

In an empty directory, with Node:

```sh
npm i --ignore-scripts @novnc/novnc@1.7.0 esbuild@0.28.2
npx esbuild node_modules/@novnc/novnc/core/rfb.js --bundle --format=esm --minify --target=es2022 \
    --legal-comments=inline --outfile=novnc.min.js
node -e "const z=require('zlib'),f=require('fs');f.writeFileSync('novnc.min.js.gz',z.gzipSync(f.readFileSync('novnc.min.js'),{level:9}))"
```

The module has a top-level `await` (noVNC asks the browser what it can decode), so the page loads it with
`<script type="module">`.
