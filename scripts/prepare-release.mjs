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
  "云同步改为独立页面，集中展示同步状态、待处理事项和副本，并说明每个处理操作的结果。 / Cloud sync now has its own page with clear status, pending items, profiles and explanations of each resolution action.",
  "重新启动浏览器副本时恢复上次打开的页面。Windows Chrome 和 Firefox 同时恢复浏览器可保存的草稿、历史与滚动位置；Edge 和 Mac Chrome 恢复上次标签页。 / Restarted profiles reopen their previous pages. Windows Chrome and Firefox also restore recoverable drafts, history and scroll positions; Edge and Mac Chrome restore their previous tabs.",
  "页面恢复沿用浏览器缓存和网站规则，部分页面仍会联网刷新。 / Restored pages follow browser cache and website rules; some pages still refresh over the network.",
  "恢复标签页时，单个网址连接失败会保留错误页并继续打开其他标签；慢页面不再阻塞副本启动。 / When restoring tabs, an unavailable URL keeps its error tab while other pages continue opening; slow pages no longer block profile startup.",
  "改善 Firefox 恢复上次页面时的启动兼容性，修复窗口变化时可能出现的原生配置接口误报。 / Improves Firefox startup compatibility when reopening saved pages and fixes native configuration errors caused by window-change notifications.",
  "改善较慢设备上的浏览器连接初始化，减少握手尚未完成时的连接误报。 / Improves browser connection startup on slower devices and reduces premature handshake errors."
];
const manifest={version,notes:releaseNotes.map(note=>`- ${note}`).join('\n'),pub_date:new Date().toISOString(),platforms};
fs.writeFileSync(path.join(directory,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
files.push('latest.json');
const sums=files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,name))).digest('hex')}  ${name}`).join('\n')+'\n';
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),sums);
fs.writeFileSync(path.join(projectRoot,'.release-notes.md'),`NodeCloak ${version}\n\n${manifest.notes}\n\n已有浏览器副本和修复记录继续保留。0.5.9 及后续版本可在应用内检查更新。\n\n提供 Windows x64 安装版和便携版，以及 macOS Apple Silicon 和 Intel 安装包。\n`);
console.log(`Verified complete release ${version}; checksums and release notes written.`);
