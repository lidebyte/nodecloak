import {useEffect,useRef,useState} from 'react';
import {ArrowUpRight,ChevronDown} from 'lucide-react';
import {getLanguage} from './i18n';
import BrowserSiteIcon from './BrowserSiteIcon';
import {useBrowserSiteCatalog} from './browser-site-catalog';

export default function BrowserSiteShortcuts({disabled,onOpen}:{disabled:boolean;onOpen:(id:string,revision:number)=>void}) {
  const {catalog,ready}=useBrowserSiteCatalog();
  const [expanded,setExpanded]=useState<string|null>(null);
  const menu=useRef<HTMLDivElement>(null);
  const triggers=useRef<Map<string,HTMLButtonElement|null>>(new Map());
  useEffect(()=>{if(disabled)setExpanded(null);},[disabled]);
  useEffect(()=>{setExpanded(null);},[catalog.revision]);
  useEffect(()=>{
    if(!expanded)return;
    function outside(event:PointerEvent){if(event.target instanceof Node&&!menu.current?.contains(event.target))setExpanded(null);}
    function escape(event:KeyboardEvent){if(event.key==='Escape'){setExpanded(null);triggers.current.get(expanded!)?.focus();}}
    document.addEventListener('pointerdown',outside);
    document.addEventListener('keydown',escape);
    return()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',escape);};
  },[expanded]);
  return <div className="browser-site-shortcuts" ref={menu}>
    {catalog.groups.filter(g=>g.sites.length>0).map(group=>{
      const label=getLanguage()==='en'?group.nameEn:group.name;
      const controlId=`browser-site-${group.id}`;
      const focusFirst=()=>requestAnimationFrame(()=>menu.current?.querySelector<HTMLButtonElement>(`[id="${controlId}"] button`)?.focus());
      return <div className="browser-site-menu" key={group.id}>
        <button ref={element=>{triggers.current.set(group.id,element);}} className="button secondary" disabled={disabled||!ready} aria-haspopup="menu" aria-expanded={expanded===group.id} aria-controls={controlId} onClick={()=>setExpanded(value=>value===group.id?null:group.id)} onKeyDown={event=>{if(event.key==='ArrowDown'){event.preventDefault();setExpanded(group.id);focusFirst();}}}><BrowserSiteIcon icon={group.icon}/>{label}<ChevronDown size={15}/></button>
        {expanded===group.id&&<div id={controlId} className="profile-menu browser-site-options" role="menu" aria-label={label} onKeyDown={event=>{if(!['ArrowDown','ArrowUp','Home','End'].includes(event.key))return;event.preventDefault();const buttons=Array.from(event.currentTarget.querySelectorAll('button'));const index=buttons.indexOf(document.activeElement as HTMLButtonElement);buttons[event.key==='Home'?0:event.key==='End'?buttons.length-1:(index+(event.key==='ArrowDown'?1:-1)+buttons.length)%buttons.length]?.focus();}}>{group.sites.map(site=><button role="menuitem" key={site.id} disabled={disabled} onClick={()=>{setExpanded(null);triggers.current.get(group.id)?.focus();onOpen(site.id,catalog.revision);}}><BrowserSiteIcon icon={site.icon}/><span>{site.name}</span><ArrowUpRight size={14}/></button>)}</div>}
      </div>;
    })}
  </div>;
}
