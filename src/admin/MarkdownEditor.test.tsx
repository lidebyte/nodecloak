// @vitest-environment jsdom
import {act,useState} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {beforeEach,afterEach,expect,it} from 'vitest';
import MarkdownEditor from './MarkdownEditor';
let host:HTMLDivElement,root:Root;
function Editor({initial='Ready'}:{initial?:string}){const [value,setValue]=useState(initial);return <MarkdownEditor value={value} onChange={setValue}/>;}
beforeEach(()=>{host=document.createElement('div');document.body.append(host);root=createRoot(host);Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();});
it('formats selected text, updates live preview and retains source when switching modes',async()=>{
 await act(async()=>root.render(<Editor/>));
 const textarea=host.querySelector('textarea')!;
 textarea.setSelectionRange(0,5);
 await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="加粗"]')!.click());
 expect(textarea.value).toBe('**Ready**');
 expect(host.querySelector('.markdown-preview strong')?.textContent).toBe('Ready');
 const mode=(name:string)=>Array.from(host.querySelectorAll<HTMLButtonElement>('.markdown-modes button')).find(b=>b.textContent===name)!;
 await act(async()=>mode('预览').click());
 expect(host.querySelector('textarea')).toBeNull();
 expect(host.querySelector('.markdown-preview strong')?.textContent).toBe('Ready');
 await act(async()=>mode('编辑').click());
 expect(host.querySelector('textarea')?.value).toBe('**Ready**');
 expect(host.querySelector('.markdown-preview')).toBeNull();
});
it('does not exceed the announcement length limit through toolbar insertion',async()=>{
 await act(async()=>root.render(<Editor initial={'a'.repeat(2000)}/>));
 host.querySelector('textarea')!.setSelectionRange(2000,2000);
 await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="加粗"]')!.click());
 expect(host.querySelector('textarea')?.value.length).toBe(2000);
 expect(host.textContent).toContain('内容已达到字数限制');
});
