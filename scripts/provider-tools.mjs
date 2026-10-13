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
  const asset = release.assets.find((a) => a.name === assetName);
  if (!asset) throw new Error(`Missing release asset: ${repo}/${assetName}`);
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
const manifest = [];
const yt = join(dest, 'yt-dlp.exe');
if (force || !existsSync(yt))
  manifest.push(await download('yt-dlp/yt-dlp', 'yt-dlp.exe', yt));
const deno = join(dest, 'deno.exe');
if (force || !existsSync(deno)) {
  const zip = join(dest, 'deno.zip');
  manifest.push(
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
for (const binary of [yt, deno]) {
  if (readFileSync(binary).subarray(0, 2).toString() !== 'MZ')
    throw new Error(`Not a Windows executable: ${binary}`);
  execFileSync(binary, ['--version'], { stdio: 'inherit' });
}
if (manifest.length)
  writeFileSync(join(dest, 'versions.json'), JSON.stringify(manifest, null, 2));
