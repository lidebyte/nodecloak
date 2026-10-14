import {t} from './i18n';
import {Globe2,LoaderCircle,MapPin} from 'lucide-react';
import {needsRegionalControl,requiresBrowserControl,type ProfileDraft,type IpRegion} from './browser-profiles';
import {native} from './bridge';
function Choices<T extends string>({label,value,options,onChange}:{label:string;value:T;options:readonly (readonly [T,string])[];onChange:(value:T)=>void}){return <div className="region-setting"><strong>{t(label)}</strong><div className="proxy-modes" role="group" aria-label={t(label)}>{options.map(([id,text])=><button type="button" key={id} aria-pressed={value===id} className={value===id?'selected':''} onClick={()=>onChange(id)}>{t(text)}</button>)}</div></div>;}
export default function ProfileRegion({draft,onChange,match,matching,onMatch}:{draft:ProfileDraft;onChange:(draft:ProfileDraft)=>void;match:IpRegion|null;matching:boolean;onMatch:()=>void}){
  const p=draft.preferences,r=p.regional;
  const ip=r.languageMode==='ip'||r.timezoneMode==='ip'||r.locationMode==='ip';
  const setRegion=(patch:Partial<typeof r>)=>{const next={...p,regional:{...r,...patch,enabled:true}};if(needsRegionalControl(next))next.advanced={...next.advanced,resistFingerprinting:false};onChange({...draft,preferences:next});};
  return <section className="advanced-card"><h3><Globe2 size={17}/>{t("副本区域设置")}</h3><p>{t("分别设置语言、时区与定位，或按代理出口匹配。")}</p>
    <Choices label="语言" value={r.languageMode} options={[["ip","跟随 IP 匹配"],["custom","自定义"]]} onChange={languageMode=>{const next={...p,regional:{...r,languageMode}};if(languageMode==='ip')next.advanced={...next.advanced,resistFingerprinting:false};onChange({...draft,preferences:next});}}/>
    {r.languageMode==='custom'&&<div className="profile-form region-single"><label>{t("浏览器语言")}<input aria-label={t("浏览器语言")} value={p.language} placeholder="en-US,en" onChange={e=>onChange({...draft,preferences:{...p,language:e.target.value}})}/><small className="profile-field-hint">{t("多个语言用英文逗号分隔，第一个为首选语言。")}{t(draft.browser==='firefox'&&'独立区域模式的 Firefox 使用首选语言。')}</small></label></div>}
    <Choices label="时区" value={r.timezoneMode} options={[["system","跟随电脑"],["ip","跟随 IP 匹配"],["custom","自定义"]]} onChange={timezoneMode=>setRegion({timezoneMode})}/>
    {r.timezoneMode==='custom'&&<div className="profile-form region-single"><label>{t('自定义时区')}<input aria-label={t('自定义时区')} value={r.timezone} placeholder="America/Los_Angeles" onChange={e=>setRegion({timezone:e.target.value})}/><small className="profile-field-hint">{t('使用 IANA 时区名称，自动计算夏令时。')}</small></label></div>}
    <Choices label="地理位置权限" value={p.advanced.location} options={[["ask","询问"],["allow","允许"],["block","禁用"]]} onChange={location=>onChange({...draft,preferences:{...p,advanced:{...p.advanced,location}}})}/>
    <Choices label="地理位置来源" value={r.locationMode} options={[["system","浏览器实际位置"],["ip","跟随 IP 匹配"],["custom","自定义"]]} onChange={locationMode=>setRegion({locationMode})}/>
    {r.locationMode==='custom'&&<div className="profile-form">{([['latitude','纬度',-90,90],['longitude','经度',-180,180],['accuracy','定位精度（米）',1,100000]] as const).map(([key,label,min,max])=><label key={key}>{t(label)}<input aria-label={t(label)} type="number" min={min} max={max} step={key==='accuracy'?1:'any'} value={r[key]} onChange={e=>setRegion({[key]:e.target.value===''?NaN:Number(e.target.value)})}/></label>)}</div>}
    {p.advanced.location==='allow'&&r.locationMode==='system'&&<p className="profile-field-hint">{t("允许定位时，网站可使用浏览器提供的实际位置。")}</p>}
    {p.advanced.location==='block'&&<p className="profile-field-hint">{t("启动后将禁用定位。Chrome / Edge 已保存的定位授权也会清除，改回询问后需重新授权。")}</p>}
    {ip&&<div className="region-match"><button type="button" className="button secondary compact" onClick={onMatch} disabled={matching}>{matching?<LoaderCircle size={14} className="spin"/>:<MapPin size={14}/>}{t("匹配当前 IP")}</button>{match&&<div role="status"><strong>{t(match.country)} · {t(match.city)} · {t(match.ip)}{t(!native&&'（演示）')}</strong><small>{t(match.language)}</small></div>}<p className="profile-field-hint">{t("按代理出口国家匹配常用语言；启动时会在浏览器内再次确认。按域名分流时，检测服务与目标网站可能使用不同出口。")}</p></div>}
    {requiresBrowserControl(draft.browser,p)&&<div className="advanced-caution">{t("独立区域设置会启用浏览器原生自动化接口，网页可读取到自动化模式标记。")}{t(p.fingerprint?.enabled?"稳定指纹也使用本地控制接口。关闭稳定指纹并使用自定义语言后，可恢复普通启动模式。":"若网页验证反复失败，请改为自定义语言后重启副本。")}</div>}
    {requiresBrowserControl(draft.browser,p)&&<p className="profile-field-hint">{t("区域设置由此副本的本地浏览器接口应用。请保持 NodeCloak 后台运行。")}{t(draft.browser==='firefox'&&'Firefox 需更新至支持区域接口的版本；严格指纹保护已关闭。')}</p>}
  </section>;
}
