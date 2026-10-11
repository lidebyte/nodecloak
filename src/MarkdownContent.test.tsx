// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import MarkdownContent,{markdownSummary} from './MarkdownContent';
let host:HTMLDivElement,root:Root;
beforeEach(()=>{host=document.createElement('div');document.body.append(host);root=createRoot(host);Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();});
it('renders headings, lists, emphasis, tables, tasks and code without showing Markdown syntax',async()=>{
 await act(async()=>root.render(<MarkdownContent>{'## Update\n\n**Ready**\n\n- One\n- Two\n\n| Name | State |\n| --- | --- |\n| Sync | Ready |\n\n- [x] Done\n\n```sh\nnpm run dev\n```'}</MarkdownContent>));
 expect(host.querySelector('h2')?.textContent).toBe('Update');
 expect(host.querySelector('strong')?.textContent).toBe('Ready');
 expect(host.querySelectorAll('li')).toHaveLength(3);
 expect(host.querySelector('table td')?.textContent).toBe('Sync');
 expect(host.querySelector<HTMLInputElement>('input[type=checkbox]')?.disabled).toBe(true);
 expect(host.querySelector('pre code')?.textContent).toContain('npm run dev');
});
it('ignores raw HTML and rejects executable, local, credential-bearing and data URLs',async()=>{
 await act(async()=>root.render(<MarkdownContent>{'<script>alert(1)</script>\n\n[Bad](javascript:alert%281%29) [Local](file:///etc/passwd) [Private](https://127.0.0.1/) [Secret](https://user:pass@example.com)\n\n![Bad](data:image/svg+xml,test)'}</MarkdownContent>));
 expect(host.querySelector('script')).toBeNull();
 expect(host.querySelector('a[href]')).toBeNull();
 expect(host.querySelector('img')).toBeNull();
});
it('opens valid links through the native callback or a protected browser tab',async()=>{
 const open=vi.fn();
 await act(async()=>root.render(<MarkdownContent onOpenLink={open}>{'[NodeCloak](https://nodecloak.com)'}</MarkdownContent>));
 const link=host.querySelector('a')!;
 expect(link.rel).toBe('noopener noreferrer');
 expect(link.target).toBe('_blank');
 await act(async()=>link.click());
 expect(open).toHaveBeenCalledWith('https://nodecloak.com/');
});
it('creates readable summaries from formatted announcements',()=>{
 expect(markdownSummary('## Update\n\n**Ready** at [NodeCloak](https://nodecloak.com)')).toBe('Update Ready at NodeCloak');
 expect(markdownSummary('<script>bad()</script>\n\nNormal')).toBe('Normal');
 expect(markdownSummary('- One\n- Two\n\n| Name | State |\n| --- | --- |\n| Sync | Ready |')).toBe('One Two Name State Sync Ready');
});
