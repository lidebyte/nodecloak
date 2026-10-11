import {useEffect,useId,useLayoutEffect,useRef,useState,type KeyboardEvent,type ReactNode} from 'react';
import {createPortal} from 'react-dom';
import {Check,ChevronDown,Search} from 'lucide-react';
import {t} from './i18n';

export interface SelectOption {value:string;label:string;icon?:ReactNode}
interface Props {
  id?:string; label:string; value:string; options:SelectOption[];
  onChange:(value:string)=>void; onOpen?:()=>void; disabled?:boolean;
  className?:string; searchable?:boolean; placeholder?:string;
  invalid?:boolean; describedBy?:string;
  menuMinWidth?:number; noWrap?:boolean;
}

export default function SelectMenu({id,label,value,options,onChange,onOpen,disabled=false,className='',searchable=false,placeholder,invalid,describedBy,menuMinWidth=178,noWrap=false}:Props) {
  const uid=useId(),listId=`${uid}-options`;
  const trigger=useRef<HTMLButtonElement>(null),panel=useRef<HTMLDivElement>(null),search=useRef<HTMLInputElement>(null);
  const [open,setOpen]=useState(false),[query,setQuery]=useState(''),[active,setActive]=useState(0);
  const [placement,setPlacement]=useState({left:0,top:0,width:178,maxHeight:320});
  const [host,setHost]=useState<HTMLElement|null>(null);
  const selected=options.find(option=>option.value===value);
  const filtered=options.filter(option=>`${option.label} ${option.value}`.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
  const currentIndex=Math.min(active,Math.max(0,filtered.length-1));
  const typeahead=useRef({text:'',time:0});

  function close(restore=false){setOpen(false);if(restore)trigger.current?.focus();}
  function expand(last=false){
    if(disabled)return;
    setHost(trigger.current?.closest('dialog')||document.body);
    setQuery('');setActive(last?Math.max(0,options.length-1):Math.max(0,options.findIndex(option=>option.value===value)));
    setOpen(true);onOpen?.();
  }
  function choose(option:SelectOption){onChange(option.value);close(true);}
  function keyboard(event:KeyboardEvent){
    if(event.key==='Tab'){close();return;}
    if(event.key==='Escape'&&open){event.preventDefault();event.stopPropagation();close(true);return;}
    if(event.key==='ArrowDown'||event.key==='ArrowUp'){
      event.preventDefault();
      if(!open){expand(event.key==='ArrowUp');return;}
      setActive(index=>(index+(event.key==='ArrowDown'?1:-1)+filtered.length)%Math.max(1,filtered.length));return;
    }
    if(open&&(event.key==='Home'||event.key==='End')&&event.target!==search.current){event.preventDefault();setActive(event.key==='Home'?0:Math.max(0,filtered.length-1));return;}
    if(event.key==='Enter'||(event.key===' '&&event.target!==search.current)){
      event.preventDefault();if(!open)expand();else if(filtered[currentIndex])choose(filtered[currentIndex]);return;
    }
    if(open&&!searchable&&event.key.length===1&&!event.ctrlKey&&!event.metaKey&&!event.altKey){
      event.preventDefault();const now=Date.now();
      typeahead.current={text:(now-typeahead.current.time<600?typeahead.current.text:'')+event.key.toLocaleLowerCase(),time:now};
      const index=filtered.findIndex(option=>option.label.toLocaleLowerCase().startsWith(typeahead.current.text));
      if(index>=0)setActive(index);
    }
  }
  useEffect(()=>{if(disabled)setOpen(false);},[disabled]);
  useLayoutEffect(()=>{
    if(!open)return;
    function position(){
      const rect=trigger.current?.getBoundingClientRect();if(!rect)return;
      const width=Math.min(Math.max(rect.width,searchable?320:178,menuMinWidth),window.innerWidth-24);
      const rows=panel.current?.querySelector('.select-menu-list');
      const searchHeight=panel.current?.querySelector('.select-menu-search')?.getBoundingClientRect().height||0;
      const desired=Math.min(320,(rows?.scrollHeight||40)+searchHeight+18);
      const below=window.innerHeight-rect.bottom-20,above=rect.top-20;
      const up=below<desired&&above>below;
      const maxHeight=Math.max(40,Math.min(320,up?above:below));
      setPlacement({left:Math.max(12,Math.min(rect.left,window.innerWidth-width-12)),top:up?Math.max(12,rect.top-Math.min(desired,maxHeight)-8):rect.bottom+8,width,maxHeight});
    }
    position();
    const observer=new ResizeObserver(position);if(trigger.current)observer.observe(trigger.current);
    function scroll(event:Event){if(event.target instanceof Node&&panel.current?.contains(event.target))return;position();}
    window.addEventListener('resize',position);window.addEventListener('scroll',scroll,true);
    return()=>{observer.disconnect();window.removeEventListener('resize',position);window.removeEventListener('scroll',scroll,true);};
  },[open,searchable,filtered.length,placement.width,menuMinWidth]);
  useEffect(()=>{
    if(!open)return;
    if(searchable)search.current?.focus();
    function outside(event:PointerEvent){if(event.target instanceof Node&&!trigger.current?.contains(event.target)&&!panel.current?.contains(event.target))close();}
    document.addEventListener('pointerdown',outside);
    return()=>document.removeEventListener('pointerdown',outside);
  },[open,searchable]);
  useEffect(()=>{if(open)panel.current?.querySelector(`[id="${listId}-${currentIndex}"]`)?.scrollIntoView({block:'nearest'});},[open,currentIndex,listId]);

  return <div className={`select-menu ${className}`}>
    <button ref={trigger} id={id} type="button" className="select-menu-trigger" role="combobox" aria-label={label}
      aria-expanded={open} aria-haspopup="listbox" aria-controls={open?listId:undefined}
      aria-activedescendant={open&&!searchable&&filtered.length?`${listId}-${currentIndex}`:undefined}
      aria-invalid={invalid} aria-describedby={describedBy} disabled={disabled}
      onClick={()=>open?close():expand()} onKeyDown={keyboard}>
      {selected?.icon}<span>{selected?.label||value||placeholder||label}</span><ChevronDown size={15}/>
    </button>
    {open&&host&&createPortal(<div ref={panel} className={`profile-menu select-menu-options${noWrap?' select-menu-nowrap':''}`} style={placement} onKeyDown={keyboard}>
      {searchable&&<div className="select-menu-search"><Search size={15}/><input ref={search} aria-label={label} placeholder={placeholder||label} value={query}
        onChange={event=>{setQuery(event.target.value);setActive(0);}} role="combobox" aria-autocomplete="list" aria-expanded="true" aria-controls={listId}
        aria-activedescendant={filtered.length?`${listId}-${currentIndex}`:undefined}/></div>}
      <div id={listId} role="listbox" aria-label={label} className="select-menu-list">
        {filtered.map((option,index)=><button key={option.value} id={`${listId}-${index}`} type="button" role="option" aria-selected={option.value===value} tabIndex={-1}
          className={`${option.value===value?'chosen ':''}${index===currentIndex?'highlighted':''}`}
          onPointerMove={()=>setActive(index)} onMouseDown={event=>event.preventDefault()} onClick={event=>{event.stopPropagation();choose(option);}}>
          {option.icon}<span>{option.label}</span><Check size={16} className="select-menu-check" aria-hidden="true"/>
        </button>)}
      </div>
      {!filtered.length&&<p className="select-menu-empty" role="status">{t('没有匹配项')}</p>}
    </div>,host)}
  </div>;
}
