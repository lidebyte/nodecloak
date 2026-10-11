// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import {AnnouncementsPanel} from './AdminApp';
let host:HTMLDivElement,root:Root;
beforeEach(()=>{host=document.createElement('div');document.body.append(host);root=createRoot(host);Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();});
const button=(name:string,scope:ParentNode=host)=>Array.from(scope.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.getAttribute('aria-label')===name||b.textContent===name)!;
const makeItems=()=>Array.from({length:21},(_,i)=>({id:`notice-${i}`,title:`Notice ${i}`,body:'## Update\n\n**Ready**',published:true,revision:1,createdAt:'2026-10-11T10:00:00Z',updatedAt:'2026-10-11T10:00:00Z',publishedAt:'2026-10-11T10:00:00Z'}));
it('requests paginated data and goes back a page after removing its final entry',async()=>{
 let items=makeItems();
 const api=vi.fn(async(path:string,method='GET')=>{
  if(method==='DELETE'){items=items.filter(i=>!path.endsWith(i.id));return {ok:true};}
  const url=new URL(path,'https://nodecloak.com'),page=Number(url.searchParams.get('page')),pageSize=Number(url.searchParams.get('pageSize'));
  return {items:items.slice((page-1)*pageSize,page*pageSize),page,pageSize,total:items.length};
 });
 await act(async()=>root.render(<AnnouncementsPanel api={api as never}/>));
 expect(api).toHaveBeenCalledWith('/admin/announcements?page=1&pageSize=10');
 expect(host.querySelectorAll('article')).toHaveLength(10);
 await act(async()=>button('下一页').click());
 expect(host.querySelector('article h3')?.textContent).toBe('Notice 10');
 await act(async()=>button('下一页').click());
 expect(host.querySelectorAll('article')).toHaveLength(1);
 expect(button('下一页').disabled).toBe(true);
 await act(async()=>button('移除').click());
 await act(async()=>button('确认',host.querySelector('[role=dialog]')!).click());
 expect(host.textContent).toContain('第 2 / 2 页');
 expect(host.querySelectorAll('article')).toHaveLength(10);
 expect(button('下一页').disabled).toBe(true);
});
it('preserves Markdown during edit, preview and save with revision checking',async()=>{
 const item=makeItems()[0];
 const api=vi.fn(async(_path:string,method='GET')=>method==='GET'?{items:[item],total:1,page:1,pageSize:10}:item);
 await act(async()=>root.render(<AnnouncementsPanel api={api as never}/>));
 await act(async()=>button('编辑').click());
 const modal=host.querySelector('[role=dialog]')!;
 expect(modal.querySelector('textarea')?.value).toBe(item.body);
 await act(async()=>button('预览',modal).click());
 expect(modal.querySelector('.markdown-preview h2')?.textContent).toBe('Update');
 expect(modal.querySelector('.markdown-preview strong')?.textContent).toBe('Ready');
 await act(async()=>modal.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true})));
 expect(api).toHaveBeenCalledWith('/admin/announcements/notice-0','PATCH',{title:item.title,body:item.body,published:true,revision:1});
 expect(host.querySelector('[role=dialog]')).toBeNull();
 expect(host.textContent).toContain('公告已保存');
});
