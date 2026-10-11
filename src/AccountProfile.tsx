import {useEffect,useRef,useState,type ReactNode} from 'react';
import {Check,KeyRound,Link2,LoaderCircle,Pencil,ShieldCheck,Unlink,X} from 'lucide-react';
import QRCode from 'qrcode';
import {t} from './i18n';
import type {Controller} from './AccountWorkspace';
import {AccountAvatar,AvatarEditor} from './AvatarEditor';

function ProfileEditDialog({title,busy,onClose,children}:{title:string;busy:boolean;onClose:()=>void;children:ReactNode}){
 const dialog=useRef<HTMLDialogElement>(null);
 useEffect(()=>{const element=dialog.current,previous=document.activeElement;element?.showModal();return()=>{element?.close();if(previous instanceof HTMLElement&&previous.isConnected)previous.focus();};},[]);
 useEffect(()=>{dialog.current?.querySelector<HTMLInputElement>('input:not(:disabled):not([readonly])')?.focus();},[title]);
 return <dialog ref={dialog} className="app-dialog account-edit-dialog" aria-labelledby="account-edit-title" aria-busy={busy} onCancel={event=>{event.preventDefault();if(!busy)onClose();}}>
  <button className="modal-close icon-button" type="button" aria-label={t('关闭面板')} disabled={busy} onClick={onClose}><X size={18}/></button><h2 id="account-edit-title">{t(title)}</h2>{children}
 </dialog>;
}

export function AccountProfile({cloud,googlePending,onGoogle}:{cloud:Controller;googlePending:boolean;onGoogle:()=>void}){
 const account=cloud.status.account!;
 const [editing,setEditing]=useState(false),[name,setName]=useState(account.name),[avatar,setAvatar]=useState(account.avatar),[file,setFile]=useState<File|null>(null);
 const [mode,setMode]=useState<'password'|'setup'|'disable'|'unlink'|'link'|'recovery'|null>(null),[currentPassword,setCurrentPassword]=useState(''),[password,setPassword]=useState(''),[confirm,setConfirm]=useState(''),[factor,setFactor]=useState('');
 const [setup,setSetup]=useState<{secret:string;uri:string}|null>(null),[qr,setQr]=useState(''),[backupCodes,setBackupCodes]=useState<string[]>([]),[saved,setSaved]=useState(false),[message,setMessage]=useState('');
 const [codesPersisted,setCodesPersisted]=useState(true);
 const [exportMessage,setExportMessage]=useState('');
 useEffect(()=>{if(!editing){setName(account.name);setAvatar(account.avatar);}},[account.name,account.avatar,editing]);
 useEffect(()=>{void cloud.act<{backupCodes?:string[];backupCodesPersisted?:boolean}>({type:'pendingTwoFactorRecovery'}).then(result=>{if(result?.backupCodes){setBackupCodes(result.backupCodes);setCodesPersisted(result.backupCodesPersisted!==false);}});},[account.id]);
 useEffect(()=>{let alive=true;setQr('');if(setup)void QRCode.toDataURL(setup.uri,{width:200,margin:2,errorCorrectionLevel:'M'}).then(value=>{if(alive)setQr(value);}).catch(()=>{if(alive)cloud.setError('无法生成验证器二维码，请使用设置密钥');});return()=>{alive=false;};},[setup]);
 useEffect(()=>{if(mode==='link'&&account.googleLinked){choose(null);setMessage('Google 已关联');}},[mode,account.googleLinked]);
 function choose(next:typeof mode){setMode(next);setCurrentPassword('');setPassword('');setConfirm('');setFactor('');setSetup(null);setMessage('');setExportMessage('');cloud.setError('');}
 function closeProfile(){setEditing(false);setName(account.name);setAvatar(account.avatar);cloud.setError('');}
 async function saveProfile(){const result=await cloud.act({type:'updateAccount',name:name.trim(),avatar});if(result!==null){setEditing(false);setMessage('资料已保存');}}
 async function security(){
  if(mode==='password'&&password!==confirm){cloud.setError('两次输入的密码不一致');return;}
  const proof={currentPassword,code:factor.trim()};
  if(mode==='setup'&&!setup){const result=await cloud.act<{secret:string;uri:string}>({type:'startTwoFactor',...proof});if(result){setSetup(result);setCurrentPassword('');setFactor('');}return;}
  if(mode==='setup'){const result=await cloud.act<{backupCodes:string[];backupCodesPersisted?:boolean}>({type:'enableTwoFactor',code:factor.trim()});if(result){setBackupCodes(result.backupCodes);setCodesPersisted(result.backupCodesPersisted!==false);setSaved(false);choose('recovery');setMessage('2FA 已开启');}return;}
  const result=await cloud.act(mode==='password'?{type:'setPassword',password,...proof}:mode==='unlink'?{type:'googleUnlink',...proof}:{type:'disableTwoFactor',...proof});
  if(result!==null){if(mode==='disable')setBackupCodes([]);const text=mode==='password'?'密码已保存':mode==='unlink'?'Google 已解除关联':'2FA 已关闭';choose(null);setMessage(text);}
 }
 async function downloadCodes(){setExportMessage('');const result=await cloud.act<{saved:boolean}>({type:'saveRecoveryFile',kind:'twoFactor'});if(result!==null)setExportMessage(result.saved?'文件已保存':'已取消保存');}
 const proofReady=(!account.hasPassword||!!currentPassword)&&(!account.twoFactorEnabled||!!factor.trim());
 return <div className="account-editor">
  <div className="account-profile-hero"><div className="account-profile-avatar"><AccountAvatar src={avatar} name={name}/><label className={`avatar-edit-button${cloud.busy?' disabled':''}`} title={t('更换头像')}><Pencil size={18}/><input type="file" aria-label={t('更换头像')} accept="image/png,image/jpeg,image/webp" disabled={cloud.busy} onChange={event=>{const value=event.target.files?.[0];if(value)setFile(value);event.target.value='';}}/></label></div>
   <h2>{account.name}<button className="icon-button" aria-label={t('编辑用户名')} disabled={cloud.busy} onClick={()=>{choose(null);setName(account.name);setEditing(true);}}><Pencil size={15}/></button></h2><span className="account-profile-handle">@{account.name}</span><small>{account.email}</small>
  </div>
  {editing&&<ProfileEditDialog title={avatar!==account.avatar?'编辑个人资料':'编辑用户名'} busy={cloud.busy} onClose={closeProfile}><form onSubmit={event=>{event.preventDefault();void saveProfile();}}>{avatar!==account.avatar&&<div className="account-edit-avatar"><AccountAvatar src={avatar} name={name}/></div>}<label className="account-field"><span>{t('用户名')}</span><input autoFocus autoComplete="nickname" value={name} onChange={event=>setName(event.target.value)} maxLength={60} required disabled={cloud.busy}/></label>{cloud.error&&<p className="profile-error" role="alert">{t(cloud.error)}</p>}<div className="account-actions">{avatar&&<button type="button" className="button secondary compact" disabled={cloud.busy} onClick={()=>setAvatar('')}>{t('移除头像')}</button>}<button type="button" className="button secondary compact" disabled={cloud.busy} onClick={closeProfile}>{t('取消')}</button><button type="submit" className="button primary compact" disabled={cloud.busy||!name.trim()}><Check size={14}/>{t(cloud.busy?'正在处理':'保存')}</button></div></form></ProfileEditDialog>}
  {file&&<AvatarEditor file={file} onSave={png=>{setAvatar(png);setEditing(true);setFile(null);}} onClose={()=>setFile(null)} onError={cloud.setError}/>}
  <section className="account-security-section"><div className="account-security-row"><KeyRound size={20}/><div><h3>{t('密码')}</h3><small>{t(account.hasPassword?'已设置':'尚未设置')}</small></div><button className="button secondary compact" disabled={cloud.busy} onClick={()=>choose('password')}>{t(account.hasPassword?'修改密码':'设置密码')}</button></div>
   {!account.hasPassword&&!account.recentAuth&&<p className="cloud-hint">{t('设置密码前请重新登录验证身份。')}{account.googleLinked&&<button className="button secondary compact" disabled={cloud.busy||googlePending} onClick={onGoogle}>{t(googlePending?'正在等待 Google 登录':'重新验证 Google')}</button>}</p>}
  </section>
  <section className="account-security-section"><div className="account-security-row"><ShieldCheck size={20}/><div><h3>{t('两步验证 (2FA)')}</h3><small>{t(account.twoFactorEnabled?'已开启':'未开启')}</small></div><button className="button secondary compact" disabled={cloud.busy||!account.hasPassword} onClick={()=>choose(account.twoFactorEnabled?'disable':'setup')}>{t(account.twoFactorEnabled?'关闭 2FA':'设置 2FA')}</button></div>
   <p className="cloud-hint">{t('Google 登录使用 Google 的身份验证；密码和邮箱验证码登录需验证 2FA。')}</p>{!account.hasPassword&&<p className="cloud-hint">{t('请先设置密码。')}</p>}
   {!!backupCodes.length&&<button className="button secondary compact" disabled={cloud.busy} onClick={()=>choose('recovery')}><KeyRound size={14}/>{t('保存 2FA 恢复码')}</button>}
  </section>
  <section className="account-security-section account-oauth"><div className="account-security-row"><Link2 size={20}/><div><h3>Google</h3><small>{t(account.googleLinked?'已关联':'未关联')}</small></div><button className="button secondary compact" disabled={cloud.busy||!!account.googleLinked&&!account.hasPassword} onClick={()=>choose(account.googleLinked?'unlink':'link')}>{account.googleLinked?<Unlink size={14}/>:<Link2 size={14}/>} {t(googlePending?'正在等待 Google 登录':account.googleLinked?'解除关联':'关联 Google')}</button></div>{account.googleLinked&&!account.hasPassword&&<p className="cloud-hint">{t('解除 Google 关联前请先设置密码。')}</p>}</section>
  {mode&&<ProfileEditDialog title={mode==='password'?account.hasPassword?'修改密码':'设置密码':mode==='setup'?'设置 2FA':mode==='disable'?'关闭 2FA':mode==='unlink'?'解除关联':mode==='link'?'关联 Google':'保存 2FA 恢复码'} busy={cloud.busy} onClose={()=>choose(null)}>
   {mode==='password'&&!account.hasPassword&&!account.recentAuth&&<p className="cloud-hint">{t('设置密码前请重新登录验证身份。')}{account.googleLinked&&<button className="button secondary compact" disabled={cloud.busy||googlePending} onClick={onGoogle}>{t(googlePending?'正在等待 Google 登录':'重新验证 Google')}</button>}</p>}
   {mode==='link'?<><p className="cloud-hint">{t('使用当前账号的邮箱完成 Google 验证。')}</p><p className="account-link-email">{account.email}</p><div className="account-actions"><button type="button" className="button secondary compact" disabled={cloud.busy} onClick={()=>choose(null)}>{t('取消')}</button><button type="button" className="button primary compact" disabled={cloud.busy||googlePending} onClick={onGoogle}>{googlePending?<LoaderCircle size={14} className="spin"/>:<Link2 size={14}/>} {t(googlePending?'正在等待 Google 登录':'关联 Google')}</button></div></>:
    mode==='recovery'?<div className="account-backup-codes"><p className="cloud-hint">{t(codesPersisted?'每个恢复码只能使用一次。确认保存前，恢复码会暂存于本机系统密钥库。':'系统密钥库暂存失败，请立即保存恢复码文件后再离开。')}</p><div>{backupCodes.map(value=><code key={value}>{value}</code>)}</div><div className="account-actions"><button type="button" className="button secondary compact" onClick={downloadCodes}>{t('保存到文件')}</button></div><label className="profile-checkbox"><input type="checkbox" checked={saved} disabled={cloud.busy} onChange={event=>setSaved(event.target.checked)}/>{t('我已安全保存恢复码')}</label><div className="account-actions"><button type="button" className="button primary compact" disabled={!saved||cloud.busy} onClick={()=>void cloud.act({type:'acknowledgeTwoFactorRecovery'}).then(result=>{if(result!==null){setBackupCodes([]);setSaved(false);choose(null);}})}><Check size={14}/>{t('完成')}</button></div></div>:securityForm()}
   {mode==='recovery'&&exportMessage&&<p className="cloud-hint" role="status">{t(exportMessage)}</p>}{cloud.error&&<p className="profile-error" role="alert">{t(cloud.error)}</p>}
  </ProfileEditDialog>}
  {message&&<p className="account-success" role="status"><Check size={14}/>{t(message)}</p>}
 </div>;
 function securityForm(){return <form className="account-security-form" onSubmit={event=>{event.preventDefault();void security();}}>
  {setup?<><p className="cloud-hint">{t('使用验证器扫描二维码，或手动输入设置密钥。')}</p>{qr&&<img className="account-totp-qr" src={qr} alt={t('验证器二维码')}/>}<label className="account-field"><span>{t('设置密钥')}</span><input readOnly value={setup.secret} autoComplete="off" spellCheck={false}/></label></>:
   account.hasPassword&&<label className="account-field"><span>{t('当前密码')}</span><input type="password" autoComplete="current-password" value={currentPassword} onChange={event=>setCurrentPassword(event.target.value)} maxLength={128} required disabled={cloud.busy}/></label>}
  {mode==='password'&&<><label className="account-field"><span>{t('新密码')}</span><input type="password" autoComplete="new-password" autoFocus={!account.hasPassword} value={password} onChange={event=>setPassword(event.target.value)} minLength={12} maxLength={128} required disabled={cloud.busy}/></label><label className="account-field"><span>{t('确认密码')}</span><input type="password" autoComplete="new-password" value={confirm} onChange={event=>setConfirm(event.target.value)} minLength={12} maxLength={128} required disabled={cloud.busy}/></label><p className="cloud-hint">{t('密码长度为 12–128 个字符。')}</p></>}
  {(setup||account.twoFactorEnabled)&&<label className="account-field"><span>{t(setup?'验证器验证码':'验证器验证码或恢复码')}</span><input autoFocus={!!setup} autoComplete="one-time-code" value={factor} onChange={event=>setFactor(event.target.value)} maxLength={64} required disabled={cloud.busy}/></label>}
  <div className="account-actions"><button className="button secondary compact" type="button" disabled={cloud.busy} onClick={()=>choose(null)}><X size={14}/>{t('取消')}</button><button className="button primary compact" type="submit" disabled={cloud.busy||(!setup&&!proofReady)||mode==='password'&&(!password||!confirm||!account.hasPassword&&!account.recentAuth)}><Check size={14}/>{t(mode==='password'?'保存密码':mode==='setup'?setup?'开启 2FA':'继续':mode==='disable'?'关闭 2FA':'解除关联')}</button></div>
 </form>;}
}
