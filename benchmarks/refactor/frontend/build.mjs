import { build } from 'esbuild';
import { compile } from 'svelte/compiler';
import { readFile, mkdir } from 'node:fs/promises';
await mkdir('dist', { recursive: true });
await build({
  entryPoints: ['entry.js'], outfile: 'dist/probe.js', bundle: true, minify: true,
  format: 'iife', conditions: ['browser'], define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'svelte', setup(b) {
    b.onLoad({ filter: /\.svelte$/ }, async ({ path }) => {
      const result = compile(await readFile(path, 'utf8'), { filename: path, generate: 'client', dev: false });
      return { contents: result.js.code, loader: 'js' };
    });
  } }]
});
