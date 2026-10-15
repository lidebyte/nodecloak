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
  "新增 MCP 自动化：可连接兼容的 AI 客户端，在明确授权的副本中管理配置、打开网页、读取页面、填写表单、点击、截图并访问指定文件夹。 / Adds MCP automation for compatible AI clients to manage explicitly authorized profiles, open pages, read content, fill forms, click, capture screenshots and access selected folders.",
  "新增远程 MCP：可将已授权设备连接到远程 AI 客户端，并按副本、网站来源、文件夹和操作范围分配权限。 / Adds remote MCP access from AI clients to authorized devices, with permissions scoped by profile, website origin, folder and operation.",
  "新增 MCP 云同步工具：可查看同步状态、预览变更、同步选定副本并处理冲突。 / Adds MCP cloud-sync tools to inspect status, preview changes, sync selected profiles and resolve conflicts.",
  "Chrome、Edge 和 Firefox 均支持 MCP 浏览器操作；敏感操作可在 NodeCloak 中确认，调用状态和结果可在任务记录中查看。 / MCP browser actions support Chrome, Edge and Firefox; sensitive actions can be confirmed in NodeCloak, with job status and results available in task history.",
  "改进副本页面控制与同步状态反馈，让 AI 自动化流程更易查看和恢复。 / Improves browser-page control and sync status feedback for clearer, recoverable AI automation workflows."
];
const manifest={version,notes:releaseNotes.map(note=>`- ${note}`).join('\n'),pub_date:new Date().toISOString(),platforms};
fs.writeFileSync(path.join(directory,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
files.push('latest.json');
const sums=files.map(name=>`${crypto.createHash('sha256').update(fs.readFileSync(path.join(directory,name))).digest('hex')}  ${name}`).join('\n')+'\n';
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),sums);
fs.writeFileSync(path.join(projectRoot,'.release-notes.md'),`NodeCloak ${version}\n\n${manifest.notes}\n\n已有浏览器副本和修复记录继续保留。0.5.9 及后续版本可在应用内检查更新。\n\n提供 Windows x64 安装版和便携版，以及 macOS Apple Silicon 和 Intel 安装包。\n`);
console.log(`Verified complete release ${version}; checksums and release notes written.`);
