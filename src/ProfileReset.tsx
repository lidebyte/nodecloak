import {useEffect,useRef,useState} from 'react';
import {CircleAlert,LoaderCircle,RotateCcw,X} from 'lucide-react';
import {resetProfile,type BrowserProfile} from './browser-profiles';
import {t} from './i18n';

export default function ProfileReset({profile,busy,onBusyChange,onClose,onComplete}:{profile:BrowserProfile;busy:boolean;onBusyChange:(value:boolean)=>void;onClose:()=>void;onComplete:()=>void}) {
 const dialog=useRef<HTMLDialogElement>(null),pending=useRef(false);
 const [resetProxy,setResetProxy]=useState(false),[working,setWorking]=useState(false),[error,setError]=useState('');
 const disabled=busy||working;
 useEffect(()=>{dialog.current?.showModal();dialog.current?.querySelector<HTMLButtonElement>('[data-reset-cancel]')?.focus();},[]);
 async function confirm(){
  if(pending.current||disabled)return;
  pending.current=true;setWorking(true);setError('');onBusyChange(true);
  try{await resetProfile(profile.id,resetProxy);onComplete();}
  catch(e){setError(e instanceof Error?e.message:String(e));}
  finally{pending.current=false;setWorking(false);onBusyChange(false);}
 }
 return <dialog ref={dialog} className="app-dialog profile-reset-dialog" aria-labelledby="profile-reset-title" aria-describedby="profile-reset-description" aria-busy={working} onCancel={e=>{e.preventDefault();if(!disabled)onClose();}}>
  <button type="button" className="modal-close" aria-label={t('关闭重置确认')} disabled={disabled} onClick={onClose}><X size={19}/></button>
  <span className="modal-feature"><RotateCcw size={25}/></span><h2 id="profile-reset-title">{t('重置这个环境？')}</h2><p className="modal-subtitle">{profile.name}</p>
  <div className="notice warning-notice" id="profile-reset-description"><CircleAlert size={17}/><p>{t('将清空此副本的全部浏览器数据，包括登录状态、Cookie、缓存、书签和历史记录，恢复默认环境配置并生成新指纹。此操作无法撤销。')}</p></div>
  <p className="profile-field-hint">{t('副本名称、浏览器类型、标签和备注保留。请先关闭此副本。')}</p>
  <p className="profile-field-hint">{t('已启用云同步时，重置后的配置与登录状态也会参与同步。')}</p>
  <label className="profile-checkbox"><input type="checkbox" checked={resetProxy} disabled={disabled} onChange={e=>setResetProxy(e.target.checked)}/>{t('同时重置代理')}</label>
  <p className="profile-field-hint">{t(resetProxy?'代理将恢复为系统代理，清除该副本当前的代理地址、认证、链式代理和 API 配置。':'保留现有代理地址、认证、链式代理和 API 配置。')}</p>
  {error&&<p className="profile-error" role="alert">{t(error)}</p>}
  {working&&<p className="profile-field-hint" role="status">{t('正在重置环境，请稍候…')}</p>}
  <div className="modal-actions"><button type="button" className="button secondary" data-reset-cancel disabled={disabled} onClick={onClose}>{t('取消')}</button><button type="button" className="button danger" disabled={disabled||profile.running} onClick={()=>void confirm()}>{working?<LoaderCircle size={15} className="spin"/>:<RotateCcw size={15}/>} {t('确认重置')}</button></div>
 </dialog>;
}
