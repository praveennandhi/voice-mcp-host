import { existsSync, mkdirSync, cpSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

if (process.platform !== 'darwin') {
  process.exit(0);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const outDir = join(root, 'src-tauri', 'bundled', 'whisper-macos');
const outBin = join(outDir, 'whisper-cli');
const outServer = join(outDir, 'whisper-server');
const requiredDylibs = [
  'libwhisper.1.dylib',
  'libggml.0.dylib',
  'libggml-cpu.0.dylib',
  'libggml-blas.0.dylib',
  'libggml-metal.0.dylib',
  'libggml-base.0.dylib',
];

if (bundleComplete()) {
  console.log(`Bundled macOS whisper engine already exists: ${outDir}`);
  signBundleIfConfigured();
  process.exit(0);
}

try {
  execFileSync('cmake', ['--version'], { stdio: 'ignore' });
} catch {
  console.error('cmake is required to build the bundled macOS whisper engine.');
  console.error('Install it on the Mac with: brew install cmake');
  process.exit(1);
}

const workDir = join(root, 'src-tauri', 'target', 'macos-whisper-build');
const archive = join(workDir, 'whisper-v1.8.4.tar.gz');
const sourceDir = join(workDir, 'whisper.cpp-1.8.4');
const buildDir = join(sourceDir, 'build');

mkdirSync(workDir, { recursive: true });
mkdirSync(outDir, { recursive: true });

if (!existsSync(sourceDir)) {
  execFileSync('curl', [
    '-L',
    'https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v1.8.4.tar.gz',
    '-o',
    archive,
  ], { stdio: 'inherit' });
  execFileSync('tar', ['-xzf', archive, '-C', workDir], { stdio: 'inherit' });
}

execFileSync('cmake', [
  '-S',
  sourceDir,
  '-B',
  buildDir,
  '-DCMAKE_BUILD_TYPE=Release',
  '-DGGML_METAL=ON',
  '-DWHISPER_BUILD_TESTS=OFF',
], { stdio: 'inherit' });

execFileSync('cmake', ['--build', buildDir, '--config', 'Release', '--target', 'whisper-cli'], {
  stdio: 'inherit',
});
execFileSync('cmake', ['--build', buildDir, '--config', 'Release', '--target', 'whisper-server'], {
  stdio: 'inherit',
});

const candidates = [
  join(buildDir, 'bin', 'whisper-cli'),
  join(buildDir, 'examples', 'cli', 'whisper-cli'),
];
const built = candidates.find(existsSync);
if (!built) {
  console.error('Built whisper-cli was not found in the expected build output.');
  process.exit(1);
}

const serverCandidates = [
  join(buildDir, 'bin', 'whisper-server'),
  join(buildDir, 'examples', 'server', 'whisper-server'),
];
const builtServer = serverCandidates.find(existsSync);
if (!builtServer) {
  console.error('Built whisper-server was not found in the expected build output.');
  process.exit(1);
}

cpSync(built, outBin);
cpSync(builtServer, outServer);
for (const dylib of requiredDylibs) {
  const source = findFile(buildDir, dylib);
  if (!source) {
    console.error(`Required whisper dependency was not found: ${dylib}`);
    process.exit(1);
  }
  cpSync(source, join(outDir, dylib), { dereference: true });
}
execFileSync('chmod', ['755', outBin], { stdio: 'inherit' });
execFileSync('chmod', ['755', outServer], { stdio: 'inherit' });
rewriteRpath(outBin);
rewriteRpath(outServer);
signBundleIfConfigured();
console.log(`Bundled macOS whisper engine: ${outDir}`);

function bundleComplete() {
  return existsSync(outBin)
    && existsSync(outServer)
    && requiredDylibs.every((name) => existsSync(join(outDir, name)));
}

function findFile(dir, name) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      const found = findFile(full, name);
      if (found) return found;
    } else if (entry === name) {
      return full;
    }
  }
  return null;
}

function rewriteRpath(binary) {
  const existing = execFileSync('otool', ['-l', binary], { encoding: 'utf8' });
  if (!existing.includes('@executable_path')) {
    execFileSync('install_name_tool', ['-add_rpath', '@executable_path', binary], { stdio: 'inherit' });
  }
}

function signBundleIfConfigured() {
  const identity = process.env.APPLE_SIGNING_IDENTITY;
  if (!identity) {
    return;
  }

  for (const dylib of requiredDylibs) {
    sign(join(outDir, dylib), identity);
  }
  sign(outBin, identity);
  sign(outServer, identity);
}

function sign(path, identity) {
  execFileSync('codesign', [
    '--force',
    '--options',
    'runtime',
    '--timestamp',
    '--sign',
    identity,
    path,
  ], { stdio: 'inherit' });
}
