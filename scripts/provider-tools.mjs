import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  rmSync,
} from 'node:fs';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

// Fetch official executables at build time, never add them as base64 source
// patches. Digest verification is required before anything is executed.
const dest = resolve('src-tauri/resources/providers');
mkdirSync(dest, { recursive: true });
const force = process.argv.includes('--force');
if (process.platform !== 'win32') {
  console.log(
    'Provider bundling currently targets Windows. Other systems can select installed yt-dlp and Deno in settings.',
  );
  process.exit(0);
}
const headers = { 'User-Agent': 'CONest-provider-builder' };
if (process.env.GITHUB_TOKEN)
  headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
async function download(repo, assetName, output) {
  const response = await fetch(
    `https://api.github.com/repos/${repo}/releases/latest`,
    { headers },
  );
  if (!response.ok) throw new Error(`${repo}: ${response.status}`);
  const release = await response.json();
  const asset = release.assets.find((a) => assetName instanceof RegExp ? assetName.test(a.name) : a.name === assetName);
  if (!asset) throw new Error(`Missing release asset: ${repo}/${assetName}`);
  assetName = asset.name;
  let digest = asset.digest?.startsWith('sha256:')
    ? asset.digest.slice(7)
    : null;
  if (!digest && repo === 'yt-dlp/yt-dlp') {
    const checks = release.assets.find((a) => a.name === 'SHA2-256SUMS');
    if (checks) {
      const sums = await fetch(checks.browser_download_url).then((r) => {
        if (!r.ok) throw new Error('Checksums unavailable');
        return r.text();
      });
      const entry = sums
        .split(/\r?\n/)
        .find(
          (line) =>
            line.trim().split(/\s+/).at(-1)?.replace(/^\*/, '') === assetName,
        );
      digest = entry?.trim().split(/\s+/)[0];
    }
  }
  if (!digest || !/^[a-f0-9]{64}$/i.test(digest))
    throw new Error(`No trustworthy SHA-256 for ${repo}/${assetName}`);
  const result = await fetch(asset.browser_download_url);
  if (!result.ok) throw new Error(`Download failed: ${result.status}`);
  const bytes = Buffer.from(await result.arrayBuffer());
  if (createHash('sha256').update(bytes).digest('hex') !== digest.toLowerCase())
    throw new Error(`Checksum mismatch: ${assetName}`);
  writeFileSync(output, bytes);
  return {
    repository: repo,
    version: release.tag_name,
    asset: assetName,
    sha256: digest,
  };
}
const versions = join(dest, 'versions.json');
const manifest = existsSync(versions)
  ? JSON.parse(readFileSync(versions, 'utf8')).filter(item => item.repository !== 'BtbN/FFmpeg-Builds')
  : [];
function remember(value) {
  const index = manifest.findIndex(
    (item) => item.repository === value.repository,
  );
  if (index >= 0) manifest[index] = value;
  else manifest.push(value);
}

const yt = join(dest, 'yt-dlp.exe');
if (force || !existsSync(yt))
  remember(await download('yt-dlp/yt-dlp', 'yt-dlp.exe', yt));
const deno = join(dest, 'deno.exe');
if (force || !existsSync(deno)) {
  const zip = join(dest, 'deno.zip');
  remember(
    await download('denoland/deno', 'deno-x86_64-pc-windows-msvc.zip', zip),
  );
  const quote = (value) => `'${value.replaceAll("'", "''")}'`;
  execFileSync(
    'powershell',
    [
      '-NoProfile',
      '-NonInteractive',
      '-Command',
      `Expand-Archive -LiteralPath ${quote(zip)} -DestinationPath ${quote(dest)} -Force`,
    ],
    { stdio: 'inherit' },
  );
  rmSync(zip, { force: true });
}
const ffmpeg = join(dest, 'ffmpeg.exe');
const ffprobe = join(dest, 'ffprobe.exe');
if (force || !existsSync(ffmpeg) || !existsSync(ffprobe) || !manifest.some(item => item.repository === 'GyanD/codexffmpeg' && item.asset.endsWith('-essentials_build.zip'))) {
  const zip = join(dest, 'ffmpeg.zip');
  remember(
    await download(
      'GyanD/codexffmpeg',
      /^ffmpeg-\d+(?:\.\d+)*-essentials_build\.zip$/,
      zip,
    ),
  );
  const unpacked = join(dest, 'ffmpeg-unpacked');
  const quote = (value) => `'${value.replaceAll("'", "''")}'`;
  const ps =
    `$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath ${quote(zip)} -DestinationPath ${quote(unpacked)} -Force; ` +
    `$binary=Get-ChildItem -LiteralPath ${quote(unpacked)} -Recurse -File -Filter ffmpeg.exe | Select-Object -First 1; if(-not $binary){throw 'FFmpeg not found'}; ` +
    `Copy-Item $binary.FullName ${quote(ffmpeg)} -Force; Copy-Item (Join-Path $binary.DirectoryName 'ffprobe.exe') ${quote(ffprobe)} -Force; ` +
    `$licenses=${quote(join(dest, 'licenses'))}; New-Item -ItemType Directory -Force $licenses | Out-Null; ` +
    `$items=@(Get-ChildItem -LiteralPath ${quote(unpacked)} -Recurse -File | Where-Object { $_.Name -match '^(LICENSE|COPYING)' }); if(-not $items.Count){throw 'FFmpeg license files missing'}; ` +
    `foreach($item in $items){$relative=$item.FullName.Substring(${quote(unpacked)}.Length+1); $target=Join-Path $licenses $relative; New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null; Copy-Item $item.FullName $target -Force}`;
  execFileSync(
    'powershell',
    ['-NoProfile', '-NonInteractive', '-Command', ps],
    { stdio: 'inherit' },
  );
  rmSync(zip, { force: true });
  rmSync(unpacked, { recursive: true, force: true });
}
// A remux-only FFmpeg cannot implement the new output codec controls.
const encoders = execFileSync(ffmpeg, ['-hide_banner', '-encoders'], {
  encoding: 'utf8',
});
for (const encoder of [
  'libx264',
  'libx265',
  'libvpx-vp9',
  'libaom-av1',
  'libmp3lame',
  'libopus',
  'aac',
  'flac',
  'pcm_s16le',
]) {
  if (!encoders.includes(encoder))
    throw new Error(`FFmpeg missing required encoder: ${encoder}`);
}
execFileSync(
  ffmpeg,
  [
    '-hide_banner',
    '-loglevel',
    'error',
    '-f',
    'lavfi',
    '-i',
    'color=size=64x64:rate=1',
    '-frames:v',
    '1',
    '-c:v',
    'libx264',
    '-f',
    'null',
    '-',
  ],
  { stdio: 'inherit' },
);
writeFileSync(
  join(dest, 'licenses', 'SOURCES.md'),
  'FFmpeg essentials build: https://github.com/GyanD/codexffmpeg\nFFmpeg source: https://github.com/FFmpeg/FFmpeg\nBuild versions and archive SHA-256: ../../provider-versions.json in the portable package.\n',
);
for (const binary of [yt, deno, ffmpeg, ffprobe]) {
  if (readFileSync(binary).subarray(0, 2).toString() !== 'MZ')
    throw new Error(`Not a Windows executable: ${binary}`);
  execFileSync(
    binary,
    [binary === ffmpeg || binary === ffprobe ? '-version' : '--version'],
    { stdio: 'inherit' },
  );
}
if (manifest.length)
  writeFileSync(join(dest, 'versions.json'), JSON.stringify(manifest, null, 2));
