import { existsSync, mkdirSync, cpSync } from 'node:fs';
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

if (existsSync(outBin) && existsSync(outServer)) {
  console.log(`Bundled macOS whisper binaries already exist: ${outDir}`);
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
execFileSync('chmod', ['755', outBin], { stdio: 'inherit' });
execFileSync('chmod', ['755', outServer], { stdio: 'inherit' });
console.log(`Bundled macOS whisper-cli: ${outBin}`);
console.log(`Bundled macOS whisper-server: ${outServer}`);
