import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {projectRoot,releaseVersion,verifyReleaseAssets} from './release-utils.mjs';
if(!process.env.RELEASE_TAG) throw new Error('A version tag is required to publish');
const version=releaseVersion();
const directory=path.join(projectRoot,'release-assets');
const files=verifyReleaseAssets(directory,version);
const names={
  'windows-x86_64':`NodeCloak_${version}_windows_x64_setup.exe`,
  'darwin-aarch64':`NodeCloak_${version}_macos_arm64_update.app.tar.gz`,
  'darwin-x86_64':`NodeCloak_${version}_macos_x64_update.app.tar.gz`,
};
const platforms={};
for(const [platform,name] of Object.entries(names)) {
  const signature=fs.readFileSync(path.join(directory,`${name}.sig`),'utf8').trim();
  const decoded=Buffer.from(signature,'base64').toString('utf8');
  if(!decoded.startsWith('untrusted comment:') || !decoded.includes('trusted comment:') || !decoded.includes(`\tversion:${version}`)) throw new Error(`Invalid or mismatched updater signature file: ${platform}`);
  platforms[platform]={url:`https://github.com/nodecloak/nodecloak/releases/download/v${version}/${name}`,signature};
}
// User-facing notices only: no admin features, server internals or build/deployment details.
const releaseNotes=[
  "修复 Firefox 副本在同步网站登录状态时无法启动的问题，保留本机数据并提供更清晰的同步提示。 / Fixes Firefox profiles failing to start while syncing website sessions, preserving local data and showing clearer sync status.",
  "改进多设备同步中的删除与恢复处理，保护本机配置，减少恢复后反复出现的冲突。 / Improves profile deletion and restoration across devices, protecting local configuration and reducing repeated conflicts after recovery.",
  "同步冲突可从副本列表、启动提示和工具栏直接处理，清楚选择保留本地、使用云端或保留两份。 / Resolve sync conflicts directly from the profile list, startup messages or toolbar, with clear choices to keep local, use cloud or keep both.",
  "冲突提示持续显示，便于处理批量启动、重新启动和保存后启动时遇到的同步问题。 / Keeps conflict messages visible and makes sync issues easier to resolve during batch starts, restarts and save-and-start actions.",
  "连接失败时保留本机修改，恢复连接后可继续同步。 / Preserves local changes through connection failures so syncing can resume after reconnection."
];
const manifest={version,notes:releaseNotes.map(note=>`- ${note}`).join('\n'),pub_date:new Date().toISOString(),platforms};
fs.writeFileSync(path.join(directory,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
files.push('latest.json');
const sums=files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,name))).digest('hex')}  ${name}`).join('\n')+'\n';
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),sums);
fs.writeFileSync(path.join(projectRoot,'.release-notes.md'),`NodeCloak ${version}\n\n${manifest.notes}\n\n已有浏览器副本和修复记录继续保留。0.5.9 及后续版本可在应用内检查更新。\n\n提供 Windows x64 安装版和便携版，以及 macOS Apple Silicon 和 Intel 安装包。\n`);
console.log(`Verified complete release ${version}; checksums and release notes written.`);
