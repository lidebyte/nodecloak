import {useEffect,useRef,useState} from 'react';
import {Fingerprint,RefreshCw,LoaderCircle} from 'lucide-react';
import {t} from './i18n';
import Toggle from './Toggle';
import type {ProfileDraft} from './browser-profiles';
import {fingerprintDefaults,regenerateFingerprintSeed,type FingerprintSettings} from './profile-fingerprint';

export default function ProfileFingerprint({draft,onChange}:{draft:ProfileDraft;onChange:(draft:ProfileDraft)=>void}) {
 const saved=draft.preferences.fingerprint,enabled=Boolean(saved?.enabled);
 const [confirm,setConfirm]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const dialog=useRef<HTMLDialogElement>(null);
 useEffect(()=>{if(confirm)dialog.current?.showModal();else dialog.current?.close();},[confirm]);
 const set=(patch:Partial<FingerprintSettings>)=>{const fingerprint={...(saved??fingerprintDefaults()),...patch};onChange({...draft,preferences:{...draft.preferences,fingerprint,advanced:fingerprint.enabled?{...draft.preferences.advanced,resistFingerprinting:false,canvasProtection:false}:draft.preferences.advanced}});};
 const regenerate=async()=>{if(busy)return;setBusy(true);setError('');try{set({seed:await regenerateFingerprintSeed()});setConfirm(false);}catch(e){setError(e instanceof Error?e.message:String(e));}finally{setBusy(false);}};
 return <section className="advanced-card fingerprint-card"><h3><Fingerprint size={17}/>{t('稳定指纹')}</h3><p>{t('每个副本拥有独立指纹，同一副本重启沿用。')}</p>
  <label className="advanced-toggle"><span><strong>{t('启用稳定指纹')}</strong><small>{t('噪音按副本与网站保持稳定，语言和时区继续使用区域设置。')}</small></span><Toggle checked={enabled} onChange={e=>set({enabled:e.target.checked})}/></label>
  {(['canvasNoise','fontNoise','webglNoise','audioNoise','mediaNoise'] as const).map((key,i)=><label className="advanced-toggle" key={key}><span><strong>{t(['Canvas 噪音','字体测量噪音','WebGL 图像噪音','音频噪音','媒体设备标识'][i])}</strong><small>{t(['在读取和导出图像时加入细微扰动。','为字体宽度测量加入稳定的细微扰动。','为 WebGL 像素读取加入细微扰动。','保护离线音频渲染和分析数据。','使用稳定的设备标识，保留真实设备与权限选择。'][i])}</small></span><Toggle disabled={!enabled} checked={Boolean(enabled&&saved?.[key])} onChange={e=>set({[key]:e.target.checked})}/></label>)}
  <div className="fingerprint-actions"><span className="profile-field-hint">{t('指纹标识')}：<code>{saved?.seed.slice(0,12)||t('开启后生成')}</code></span><button type="button" className="button secondary compact" onClick={()=>{setError('');setConfirm(true);}}><RefreshCw size={15}/>{t('重新生成指纹')}</button></div>
  <p className="profile-field-hint">{t(draft.browser==='firefox'?'保护覆盖网页和 iframe；Firefox Worker 的噪音预加载尚未覆盖。':'保护覆盖网页、iframe 和受控 Worker。')}</p>
  <dialog ref={dialog} className="app-dialog" onCancel={e=>{e.preventDefault();if(!busy)setConfirm(false);}}><span className="modal-feature"><Fingerprint size={25}/></span><h2>{t('重新生成这个副本的指纹？')}</h2><p className="modal-subtitle">{draft.name}</p><p>{t('新指纹在保存并重新启动后生效，Cookie 和网站数据保留。')}</p>{error&&<p role="alert" className="profile-error">{error}</p>}<div className="modal-actions"><button type="button" className="button secondary" disabled={busy} onClick={()=>setConfirm(false)}>{t('取消')}</button><button type="button" className="button primary" disabled={busy} onClick={()=>void regenerate()}>{busy?<LoaderCircle size={15} className="spin"/>:<RefreshCw size={15}/>} {t('确认重新生成')}</button></div></dialog>
 </section>;
}
