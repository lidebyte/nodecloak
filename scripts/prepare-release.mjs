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
  "新增稳定指纹：新建和复制副本生成独立指纹，同一副本重启保持稳定，并可重新生成指纹。 / Adds stable fingerprints: new and copied profiles receive independent identities that persist across restarts and can be regenerated.",
  "Chrome、Edge、Firefox 新增 Canvas、字体测量、WebGL、音频和媒体设备标识保护，可分别调整。 / Adds individually adjustable Canvas, font measurement, WebGL, audio and media-input identity protection for Chrome, Edge and Firefox.",
  "新增批量管理：支持批量启动、关闭、删除和恢复浏览器副本。 / Adds batch profile management: start, close, delete and restore multiple browser profiles.",
  "扩展副本环境设置：支持逻辑处理器数量、页面视口、独立隐私策略及网页功能开关。 / Expands environment settings with logical processor hints, page viewport, independent privacy policies and website feature controls.",
  "增强网站登录状态同步：支持更多分区网站数据，保留不同网站之间的数据隔离。 / Improves website-session sync with support for more partitioned website data while preserving isolation between sites.",
  "改善浏览器副本启动和区域设置应用，打开页面更稳定。 / Improves profile startup and regional settings for more reliable page navigation."
];
const manifest={version,notes:releaseNotes.map(note=>`- ${note}`).join('\n'),pub_date:new Date().toISOString(),platforms};
fs.writeFileSync(path.join(directory,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
files.push('latest.json');
const sums=files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,name))).digest('hex')}  ${name}`).join('\n')+'\n';
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),sums);
fs.writeFileSync(path.join(projectRoot,'.release-notes.md'),`NodeCloak ${version}\n\n${manifest.notes}\n\n已有浏览器副本和修复记录继续保留。0.5.9 及后续版本可在应用内检查更新。\n\n提供 Windows x64 安装版和便携版，以及 macOS Apple Silicon 和 Intel 安装包。\n`);
console.log(`Verified complete release ${version}; checksums and release notes written.`);
