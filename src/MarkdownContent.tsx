import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {unified} from 'unified';
import remarkParse from 'remark-parse';
import {toString} from 'mdast-util-to-string';
import type {Root,RootContent} from 'mdast';
import './markdown.css';

const parser=unified().use(remarkParse).use(remarkGfm);
function summaryText(node:Root|RootContent):string {
 if(['root','list','listItem','blockquote','table','tableRow'].includes(node.type)&&'children' in node)return node.children.map(summaryText).join(' ');
 return toString(node,{includeHtml:false});
}
export function markdownSummary(value:string):string {
 return summaryText(parser.parse(value)).replace(/\s+/g,' ').trim();
}
export function safeMarkdownURL(value:string):string {
 try {
  const url=new URL(value),host=url.hostname.toLowerCase();
  if(value.length>2048||url.protocol!=='https:'||url.username||url.password||(url.port&&url.port!=='443')||!host.includes('.')||host.endsWith('.')||host.includes(':')||/^\d+(?:\.\d+){3}$/.test(host))return '';
  if(['localhost','local','internal','lan','home','test','invalid','onion'].some(s=>host===s||host.endsWith('.'+s)))return '';
  return url.href;
 }catch{return '';}
}
export default function MarkdownContent({children,onOpenLink}:{children:string;onOpenLink?:(url:string)=>void}) {
 return <div className="markdown-content"><ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml urlTransform={safeMarkdownURL} components={{
  a:({href,children})=>href?<a href={href} target="_blank" rel="noopener noreferrer" onClick={onOpenLink?event=>{event.preventDefault();onOpenLink(href);}:undefined}>{children}</a>:<span>{children}</span>,
  img:({src,alt})=>src?<img src={src} alt={alt??''} loading="lazy" referrerPolicy="no-referrer"/>:<span>{alt}</span>,
  table:({children})=><div className="markdown-table"><table>{children}</table></div>,
 }}>{children}</ReactMarkdown></div>;
}
