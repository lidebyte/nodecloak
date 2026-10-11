import {invoke} from '@tauri-apps/api/core';
import {native} from './bridge';
export interface Account {id:string;name:string;email:string;avatar:string;googleLinked?:boolean;hasPassword?:boolean;twoFactorEnabled?:boolean;recentAuth?:boolean}
export interface CloudConflict {localId:string;cloudId:string;name:string;kind:'config'|'session'|'running';fields:string[];remoteRevision:number}
export interface CloudStatus {account:Account|null;endpoint?:string;deviceId?:string;deviceName?:string;fingerprint?:string;vaultCode?:string|null;approvedNeedsVerification?:boolean;initialized?:boolean;unlocked?:boolean;recoverySaved?:boolean;setupComplete?:boolean;autoSync?:boolean;sessionSync?:boolean;pullCloud?:boolean;lastSync?:string|null;lastError?:string|null;conflicts?:CloudConflict[];bindings?:{localId:string;cloudId:string;sessionPending:boolean;configPending:boolean}[]}
export interface CloudDevice {id:string;name:string;publicKey:string;fingerprint:string;status:'pending'|'approved'|'revoked';createdAt:string}
export interface CloudPreview {profiles:{id:string;name:string;browser:string;deleted:boolean;revision:number}[];localProfiles:{id:string;name:string;browser:string;eligible:boolean;selected:boolean}[]}
export type CloudAction=
 |{type:'saveRecoveryFile';kind:'vault'|'twoFactor'}
 |{type:'requestEmail';email:string}|{type:'verifyEmail';email:string;code:string}
 |{type:'passwordLogin';email:string;password:string}|{type:'verifyTwoFactor'|'enableTwoFactor';code:string}
 |{type:'cancelLogin'}|{type:'setPassword';password:string;currentPassword:string;code:string}
 |{type:'pendingTwoFactorRecovery'|'acknowledgeTwoFactorRecovery'}
 |{type:'googleUnlink'|'startTwoFactor'|'disableTwoFactor';currentPassword:string;code:string}
 |{type:'googleStart'|'googlePoll'|'refresh'|'initialize'|'pendingRecovery'|'acknowledgeRecovery'|'logout'|'devices'|'preview'|'sync'}
 |{type:'recover'|'confirmDevice';code:string}|{type:'updateAccount';name:string;avatar:string}|{type:'approveDevice';id:string;verificationCode:string}|{type:'revokeDevice';id:string}
 |{type:'setup';mode:'merge'|'upload'|'none';localIds:string[];cloudIds:string[];sessions:boolean}
 |{type:'preferences';autoSync:boolean;sessionSync:boolean;pullCloud:boolean}|{type:'resolve';id:string;kind:'config'|'session';choice:'local'|'cloud'|'both'|'skip'|'takeover'}|{type:'detach';id:string;removeLocal:boolean};
export async function cloudStatus():Promise<CloudStatus>{if(!native)return {account:null};return invoke('cloud_status');}
export async function cloudAction<T=unknown>(action:CloudAction):Promise<{status:CloudStatus;result:T}>{if(!native)throw new Error('请在桌面客户端登录');return invoke('cloud_action',{action});}
export function syncAvailability(s:CloudStatus):'login'|'unlock'|'setup'|'ready'{if(!s.account)return 'login';if(!s.unlocked)return 'unlock';if(s.recoverySaved===false||!s.setupComplete)return 'setup';return 'ready';}
export function verificationCode(fp:string):string{return /^[a-f0-9]{64}$/i.test(fp)?fp.slice(-12).toUpperCase().match(/.{4}/g)!.join(' '):'';}
export function validateAvatarFile(file:{type:string;size:number}):string|null{return !['image/png','image/jpeg','image/webp'].includes(file.type)?'头像只支持 PNG、JPEG 或 WebP':file.size>8<<20?'头像文件不能超过 8 MB':null;}
export async function avatarPNG(file:File):Promise<string>{const error=validateAvatarFile(file);if(error)throw new Error(error);const image=await createImageBitmap(file);try{const canvas=document.createElement('canvas');canvas.width=256;canvas.height=256;const context=canvas.getContext('2d');if(!context)throw new Error('无法处理头像');const size=Math.min(image.width,image.height);context.drawImage(image,(image.width-size)/2,(image.height-size)/2,size,size,0,0,256,256);return canvas.toDataURL('image/png');}finally{image.close();}}
