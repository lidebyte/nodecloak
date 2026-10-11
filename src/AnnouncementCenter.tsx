import {useCallback,useEffect,useId,useRef,useMemo,useState} from 'react';
import {createPortal} from 'react-dom';
import {Megaphone,ArrowUpRight,LoaderCircle,X} from 'lucide-react';
import {t,getDateLocale} from './i18n';
import type {Announcement} from './browser-site-catalog';
import './announcements.css';
import MarkdownContent,{markdownSummary} from './MarkdownContent';
import {invoke} from '@tauri-apps/api/core';
import {native} from './bridge';

const storageKey='nodecloak.dismissed-announcements';
const pageSize=10;
function readDismissed():string[]{try{const value=JSON.parse(localStorage.getItem(storageKey)??'[]');return Array.isArray(value)?value.filter(v=>typeof v==='string').slice(-100):[];}catch{return [];}}
export default function AnnouncementCenter({announcements,toolbarTarget}:{announcements:Announcement[];toolbarTarget:HTMLElement|null}){
 const [dismissed,setDismissed]=useState(readDismissed),[open,setOpen]=useState(false);
 const [shown,setShown]=useState(pageSize),[loadingMore,setLoadingMore]=useState(false);
 const [linkError,setLinkError]=useState('');
 const list=useRef<HTMLDivElement>(null),sentinel=useRef<HTMLDivElement>(null),pendingFrame=useRef<number|null>(null);
 const modal=useRef<HTMLDivElement>(null),trigger=useRef<HTMLButtonElement>(null),focusAfterDismiss=useRef(false),heading=useId();
 const sorted=useMemo(()=>[...announcements].sort((a,b)=>Date.parse(b.publishedAt)-Date.parse(a.publishedAt)),[announcements]);
 const hasMore=shown<sorted.length;
 const loadMore=useCallback(()=>{
  if(!open||shown>=sorted.length||pendingFrame.current!==null)return;
  setLoadingMore(true);
  // The catalog is already cached. Yield a frame before rendering the next batch.
  pendingFrame.current=requestAnimationFrame(()=>{
   pendingFrame.current=null;
   setShown(count=>Math.min(count+pageSize,sorted.length));
   setLoadingMore(false);
  });
 },[open,shown,sorted.length]);
 const openHistory=()=>{setShown(pageSize);setLoadingMore(false);setLinkError('');setOpen(true);};
 useEffect(()=>{
  if(!open)return;
  return()=>{if(pendingFrame.current!==null){cancelAnimationFrame(pendingFrame.current);pendingFrame.current=null;}};
 },[open]);
 useEffect(()=>{
  if(!open||!hasMore||!list.current||!sentinel.current||typeof IntersectionObserver==='undefined')return;
  const observer=new IntersectionObserver(entries=>{
   if(entries.some(entry=>entry.isIntersecting))loadMore();
  },{root:list.current,rootMargin:'0px 0px 80px 0px'});
  observer.observe(sentinel.current);
  return()=>observer.disconnect();
 },[open,hasMore,loadMore]);
 const latest=sorted[0],key=latest?`${latest.id}:${latest.revision}`:'';
 const visible=latest&&!dismissed.includes(key);
 useEffect(()=>{
  if(!visible&&toolbarTarget&&focusAfterDismiss.current){trigger.current?.focus();focusAfterDismiss.current=false;}
 },[visible,toolbarTarget]);
 useEffect(()=>{const update=()=>setDismissed(readDismissed());window.addEventListener('storage',update);return()=>window.removeEventListener('storage',update);},[]);
 useEffect(()=>{
  if(!open)return;
  const previous=document.body.style.overflow;document.body.style.overflow='hidden';
  modal.current?.querySelector<HTMLButtonElement>('button')?.focus();
  const keyboard=(event:KeyboardEvent)=>{
   if(event.key==='Escape'){event.preventDefault();setOpen(false);}
   if(event.key==='Tab'){
    const buttons=modal.current?.querySelectorAll<HTMLElement>('button,a[href],[tabindex="0"]');
    if(!buttons?.length)return;
    const first=buttons[0],last=buttons[buttons.length-1];
    if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus();}
    else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}
   }
  };
  document.addEventListener('keydown',keyboard);
  return()=>{document.body.style.overflow=previous;document.removeEventListener('keydown',keyboard);trigger.current?.focus();};
 },[open]);
 if(!latest)return null;
 const dismiss=()=>{focusAfterDismiss.current=true;const next=[...dismissed.filter(v=>v!==key),key].slice(-100);setDismissed(next);try{localStorage.setItem(storageKey,JSON.stringify(next));}catch{/* Dismissal still lasts for this window. */}};
 return <>
  {visible?<div className="announcement-banner" role="status">
   <Megaphone size={16} aria-hidden/><div className="announcement-summary"><strong>{latest.title}</strong><span>{markdownSummary(latest.body)}</span></div>
   <button ref={trigger} className="announcement-more" aria-label={t('查看全部公告')} aria-haspopup="dialog" aria-expanded={open} onClick={openHistory}>{t('查看更多')}<ArrowUpRight size={14} aria-hidden/></button>
   <button className="announcement-dismiss icon-button" aria-label={t('关闭公告')} title={t('关闭公告')} onClick={dismiss}><X size={16}/></button>
  </div>:toolbarTarget&&createPortal(<button ref={trigger} className="icon-button announcement-trigger" title={t('公告')} aria-label={t('查看全部公告')} aria-haspopup="dialog" aria-expanded={open} onClick={openHistory}><Megaphone size={18} aria-hidden/></button>,toolbarTarget)}
  {open&&<div className="announcements-backdrop" onMouseDown={event=>{if(event.target===event.currentTarget)setOpen(false);}}><div ref={modal} className="announcements-dialog" role="dialog" aria-modal="true" aria-labelledby={heading}>
   <header><div><span className="eyebrow">NODECLOAK / NEWS</span><h2 id={heading}>{t('公告')}</h2></div><button className="icon-button" aria-label={t('关闭面板')} onClick={()=>setOpen(false)}><X size={18}/></button></header>
   <div ref={list} className="announcements-list" tabIndex={0} aria-label={t('公告')} aria-busy={loadingMore} onScroll={event=>{const element=event.currentTarget;if(element.scrollHeight-element.scrollTop-element.clientHeight<=80)loadMore();}}>
    {sorted.slice(0,shown).map(item=><article key={item.id}><time dateTime={item.publishedAt}>{new Date(item.publishedAt).toLocaleString(getDateLocale(),{dateStyle:'medium',timeStyle:'short'})}</time><h3>{item.title}</h3><MarkdownContent onOpenLink={native?url=>{setLinkError('');void invoke('open_announcement_link',{url}).catch(()=>setLinkError(t('无法打开链接，请重试。')));}:undefined}>{item.body}</MarkdownContent></article>)}
    <div ref={sentinel} className="announcements-pagination" role="status" aria-live="polite">{loadingMore?<><LoaderCircle size={14} className="spin" aria-hidden/>{t('正在加载更多公告…')}</>:t(hasMore?'向下滚动加载更多':'已显示全部公告')}</div>
   </div>
   {linkError&&<p className="announcement-link-error" role="alert">{linkError}</p>}
  </div></div>}
 </>;
}
