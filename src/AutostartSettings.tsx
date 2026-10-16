import {useEffect,useState} from 'react';
import {LoaderCircle,Power,RefreshCw} from 'lucide-react';
import {getAutostartEnabled,setAutostartEnabled} from './bridge';
import {t} from './i18n';

export default function AutostartSettings(){
  const [enabled,setEnabled]=useState<boolean|null>(null);
  const [pending,setPending]=useState(false);
  const [error,setError]=useState('');
  async function read(){
    setPending(true);setError('');
    try{setEnabled(await getAutostartEnabled());}catch(error){setError(String(error));}finally{setPending(false);}
  }
  useEffect(()=>{let cancelled=false;void getAutostartEnabled().then(value=>{if(!cancelled)setEnabled(value);}).catch(error=>{if(!cancelled)setError(String(error));});return()=>{cancelled=true;};},[]);
  async function change(value:boolean){
    if(pending||enabled===null)return;
    setPending(true);setError('');
    try{setEnabled(await setAutostartEnabled(value));}
    catch(error){setError(String(error));try{setEnabled(await getAutostartEnabled());}catch{/* Keep the last confirmed state. */}}
    finally{setPending(false);}
  }
  return <section className="settings-card" aria-label={t("启动设置")}>
    <h3 style={{display:'flex',alignItems:'center',gap:10}}><Power size={16}/>{t("启动设置")}</h3>
    <div className="settings-line"><div><strong>{t("开机自动启动")}</strong><p>{t("登录电脑后自动打开 NodeCloak。")}</p></div><label className="pending-toggle"><input type="checkbox" role="switch" aria-label={t("开机自动启动")} checked={enabled??false} disabled={pending||enabled===null} onChange={event=>void change(event.target.checked)}/><span className="switch"/></label></div>
    {enabled===null&&!error&&<p role="status"><LoaderCircle className="spin" size={14}/>{t("正在读取启动设置…")}</p>}
    {error&&<div className="update-status"><p role="alert">{t(error)}</p><button className="button secondary compact" disabled={pending} onClick={()=>void read()}><RefreshCw size={14} className={pending?'spin':''}/>{t("重新读取")}</button></div>}
  </section>;
}
