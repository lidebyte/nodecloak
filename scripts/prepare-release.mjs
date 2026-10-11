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
  "新增副本分享：输入朋友的用户名和邮箱发送邀请，可设置有效期、调整期限并随时撤销。 / Share a browser profile by entering a friend's username and email; set or change its expiry and revoke access at any time.",
  "朋友接受邀请后才能使用；未授权设备可在接受时直接授权，无需进入云端同步设置。 / Friends must accept an invitation before use; authorize a device directly when accepting, without opening cloud sync settings.",
  "分享默认包含浏览器配置，可选择附带登录状态。朋友使用发送时的独立快照，不会自动同步或修改你的原副本；到期或撤销后停止使用。 / Shares include browser configuration by default, with optional sign-in data. Friends use an independent snapshot taken when sent; it does not sync with or modify your original profile, and access stops after expiry or revocation.",
  "共享页面集中显示收到和发出的邀请，支持接受、拒绝与管理；移除顶部介绍卡片，让列表更直接。 / Manage received and sent invitations in the Sharing page, including acceptance, decline and expiry; the intro card has been removed for a simpler list.",
  "更新 Google 登录图标，并让首页顶部卡片正确跟随浅色和深色主题。 / Updated the Google sign-in icon and made the homepage banner follow light and dark themes."
];
const manifest={version,notes:releaseNotes.map(note=>`- ${note}`).join('\n'),pub_date:new Date().toISOString(),platforms};
fs.writeFileSync(path.join(directory,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
files.push('latest.json');
const sums=files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,name))).digest('hex')}  ${name}`).join('\n')+'\n';
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),sums);
fs.writeFileSync(path.join(projectRoot,'.release-notes.md'),`NodeCloak ${version}\n\n${manifest.notes}\n\n已有浏览器副本和修复记录继续保留。0.5.9 及后续版本可在应用内检查更新。\n\n提供 Windows x64 安装版和便携版，以及 macOS Apple Silicon 和 Intel 安装包。\n`);
console.log(`Verified complete release ${version}; checksums and release notes written.`);
