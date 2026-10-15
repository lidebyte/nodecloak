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
  "新增链式代理：可通过本机代理或软路由连接副本代理，支持 HTTP、HTTPS 和 SOCKS5 前置代理。 / Adds proxy chaining through a local proxy or LAN router, with HTTP, HTTPS and SOCKS5 first hops.",
  "新增 API 提取代理：支持返回 IP:端口 的接口，每次启动提取代理，并在本次运行中保持地址一致。 / Adds API proxy sources returning IP:port, extracting an endpoint at startup and keeping it consistent during the session.",
  "扩展代理导入：支持 主机:端口:用户名:密码 格式，方便粘贴代理商提供的连接信息。 / Accepts host:port:username:password proxy exports for convenient provider configuration.",
  "优化 Chrome、Edge、Firefox 的手动启动与 MCP 自动化协作，改善稳定指纹及区域设置的登录兼容性。 / Improves manual startup and MCP automation coordination across Chrome, Edge and Firefox, including login compatibility with stable fingerprints and regional settings.",
  "修复登录新标签页自动关闭及持续加载问题，增强连接恢复、网站登录状态同步与错误提示。 / Fixes login tabs closing or remaining loading, and improves connection recovery, website-session sync and error messages.",
  "启动浏览器副本时显示加载状态，便于查看启动进度。 / Shows a loading state while browser profiles start."
];
const manifest={version,notes:releaseNotes.map(note=>`- ${note}`).join('\n'),pub_date:new Date().toISOString(),platforms};
fs.writeFileSync(path.join(directory,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
files.push('latest.json');
const sums=files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,name))).digest('hex')}  ${name}`).join('\n')+'\n';
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),sums);
fs.writeFileSync(path.join(projectRoot,'.release-notes.md'),`NodeCloak ${version}\n\n${manifest.notes}\n\n已有浏览器副本和修复记录继续保留。0.5.9 及后续版本可在应用内检查更新。\n\n提供 Windows x64 安装版和便携版，以及 macOS Apple Silicon 和 Intel 安装包。\n`);
console.log(`Verified complete release ${version}; checksums and release notes written.`);
