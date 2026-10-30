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
  "修复开启登录同步后，部分本机副本因数据读取失败而无法启动的问题；读取失败时保留数据并允许打开本人副本。 / Fixed owned local profiles failing to open when sign-in sync cannot read browser data. Local data is preserved and the profile can still open.",
  "减少浏览器临时文件占用造成的同步失败；遇到短暂读取错误时自动重试。 / Reduced sync failures caused by browser temporary files and added a retry for transient read errors.",
  "完善启动日志和同步提示，显示具体读取操作与系统错误码；同步失败时不会上传不完整的登录数据。 / Improved launch logs and sync messages with the failing operation and system error code. Incomplete sign-in data is never uploaded after a failed capture."
];
const manifest={version,notes:releaseNotes.map(note=>`- ${note}`).join('\n'),pub_date:new Date().toISOString(),platforms};
fs.writeFileSync(path.join(directory,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
files.push('latest.json');
const sums=files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,name))).digest('hex')}  ${name}`).join('\n')+'\n';
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),sums);
fs.writeFileSync(path.join(projectRoot,'.release-notes.md'),`NodeCloak ${version}\n\n${manifest.notes}\n\n已有浏览器副本和修复记录继续保留。0.5.9 及后续版本可在应用内检查更新。\n\n提供 Windows x64 安装版和便携版，以及 macOS Apple Silicon 和 Intel 安装包。\n`);
console.log(`Verified complete release ${version}; checksums and release notes written.`);
