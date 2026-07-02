// Run from a scratch dir where `@paper-design/shaders` is installed:
//   sfw npm install @paper-design/shaders
//   node <path>/extract.mjs <path>/shaders
import { writeFileSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import * as paper from '@paper-design/shaders';

const outDir = process.argv[2];
mkdirSync(outDir, { recursive: true });

function write(name, src, file) {
  if (typeof src !== 'string') throw new Error(`missing export: ${name}`);
  if (src.includes('${')) throw new Error(`unresolved template in ${name}`);
  writeFileSync(join(outDir, file), src);
  console.log(`wrote ${file} (${src.length} bytes)`);
}

write('meshGradientFragmentShader', paper.meshGradientFragmentShader, 'mesh_gradient.frag');

let vert = paper.vertexShaderSource ?? paper.vertexShader;
if (typeof vert !== 'string') {
  // The shared vertex shader (needed by mesh-gradient for v_objectUV) isn't
  // re-exported from the package root's export map. Resolve the package's
  // real on-disk entry and import the internal module directly by filesystem
  // path, bypassing the "exports" map restriction on subpath specifiers.
  const require = createRequire(import.meta.url);
  const entry = require.resolve('@paper-design/shaders');
  const vertexModulePath = join(dirname(entry), 'vertex-shader.js');
  const vertexModule = await import(pathToFileURL(vertexModulePath).href);
  vert = vertexModule.vertexShaderSource ?? vertexModule.vertexShader;
}
if (typeof vert !== 'string') {
  throw new Error('shared vertex shader not exported from package root — ' +
    'mesh-gradient needs v_objectUV; locate the vertex export before proceeding');
}
write('vertexShaderSource', vert, 'vertex.vert');
