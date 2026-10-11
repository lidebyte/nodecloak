import {useEffect,useRef,useState} from 'react';
import {ArrowUpRight,Bot,ChevronDown,ShoppingBag} from 'lucide-react';
import sites from './browser-sites.json';
import {t} from './i18n';

export default function BrowserSiteShortcuts({disabled,onOpen}:{disabled:boolean;onOpen:(id:string)=>void}) {
  const [expanded,setExpanded]=useState<'ai'|'commerce'|null>(null);
  const menu=useRef<HTMLDivElement>(null);
  const triggers=useRef<Record<string,HTMLButtonElement|null>>({});
  useEffect(()=>{if(disabled)setExpanded(null);},[disabled]);
  useEffect(()=>{
    if(!expanded)return;
    function outside(event:PointerEvent){if(event.target instanceof Node&&!menu.current?.contains(event.target))setExpanded(null);}
    function escape(event:KeyboardEvent){if(event.key==='Escape'){setExpanded(null);triggers.current[expanded!]?.focus();}}
    document.addEventListener('pointerdown',outside);
    document.addEventListener('keydown',escape);
    return()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',escape);};
  },[expanded]);
  return <div className="browser-site-shortcuts" ref={menu}>
    {(['ai','commerce'] as const).map(group=>{
      const Icon=group==='ai'?Bot:ShoppingBag;
      const label=group==='ai'?'AI':t('电商网站');
      return <div className="browser-site-menu" key={group}>
        <button ref={element=>{triggers.current[group]=element;}} className="button secondary" disabled={disabled} aria-expanded={expanded===group} aria-controls={`${group}-sites`} onClick={()=>setExpanded(value=>value===group?null:group)}><Icon size={16}/>{label}<ChevronDown size={15}/></button>
        {expanded===group&&<div id={`${group}-sites`} className="profile-menu browser-site-options" role="group" aria-label={label}>{sites.filter(site=>site.group===group).map(site=><button key={site.id} disabled={disabled} onClick={()=>{setExpanded(null);triggers.current[group]?.focus();onOpen(site.id);}}><Icon size={16}/>{site.name}<ArrowUpRight size={14}/></button>)}</div>}
      </div>;
    })}
  </div>;
}
