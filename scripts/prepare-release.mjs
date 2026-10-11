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
  "新增 Google 和邮箱登录，可修改头像与用户名，并设置密码和两步验证。 / Sign in with Google or email, edit your avatar and name, and set up a password and two-factor authentication.",
  "新增端到端加密云同步，可同步浏览器副本配置和可迁移的网站登录状态，选择自动或手动同步，并处理设备间的冲突。 / Sync browser profiles and transferable website sign-in sessions with end-to-end encryption, automatic or manual sync, and conflict resolution.",
  "AI 和电商网站菜单使用各站点的对应图标，查找和打开网站更方便。 / Recognize and open sites more easily with individual icons in the AI and shopping menus.",
  "公告支持关闭和查看历史，向下滚动可继续加载更多内容。 / Dismiss announcements, browse their history, and scroll to load more.",
  "优化个人资料、偏好设置和下拉菜单，终端工作目录可直接选择文件夹。 / Improved profile and preferences pages and dropdown menus; choose terminal working folders with a folder picker."
];
const manifest={version,notes:releaseNotes.map(note=>`- ${note}`).join('\n'),pub_date:new Date().toISOString(),platforms};
fs.writeFileSync(path.join(directory,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
files.push('latest.json');
const sums=files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,name))).digest('hex')}  ${name}`).join('\n')+'\n';
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),sums);
fs.writeFileSync(path.join(projectRoot,'.release-notes.md'),`NodeCloak ${version}\n\n${manifest.notes}\n\n已有浏览器副本和修复记录继续保留。0.5.9 及后续版本可在应用内检查更新。\n\n提供 Windows x64 安装版和便携版，以及 macOS Apple Silicon 和 Intel 安装包。\n`);
console.log(`Verified complete release ${version}; checksums and release notes written.`);
