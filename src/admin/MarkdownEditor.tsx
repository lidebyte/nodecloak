import {useId,useLayoutEffect,useRef,useState} from 'react';
import {Bold,Italic,Heading2,List,ListOrdered,Link,Quote,Code2,Table2,Image} from 'lucide-react';
import MarkdownContent from '../MarkdownContent';
type Mode='edit'|'split'|'preview';
const tools=[
 {label:'加粗',icon:Bold,before:'**',after:'**',sample:'文字'},
 {label:'斜体',icon:Italic,before:'*',after:'*',sample:'文字'},
 {label:'标题',icon:Heading2,before:'## ',after:'\n',sample:'标题',block:true},
 {label:'无序列表',icon:List,before:'- ',after:'\n',sample:'列表项',block:true},
 {label:'有序列表',icon:ListOrdered,before:'1. ',after:'\n',sample:'列表项',block:true},
 {label:'链接',icon:Link,before:'[',after:'](https://nodecloak.com)',sample:'链接文字'},
 {label:'引用',icon:Quote,before:'> ',after:'\n',sample:'引用文字',block:true},
 {label:'代码块',icon:Code2,before:'```\n',after:'\n```\n',sample:'代码',block:true},
 {label:'表格',icon:Table2,before:'',after:'',sample:'| 项目 | 说明 |\n| --- | --- |\n| 项目一 | 内容 |\n',block:true},
 {label:'图片',icon:Image,before:'![',after:'](https://nodecloak.com/image.png)',sample:'图片说明'},
];
export default function MarkdownEditor({value,onChange,disabled=false,maxLength=2000}:{value:string;onChange:(value:string)=>void;disabled?:boolean;maxLength?:number}){
 const [mode,setMode]=useState<Mode>('split'),[limitError,setLimitError]=useState(false);
 const textarea=useRef<HTMLTextAreaElement>(null),selection=useRef<[number,number]|null>(null),id=useId();
 useLayoutEffect(()=>{if(selection.current&&textarea.current){textarea.current.focus();textarea.current.setSelectionRange(...selection.current);selection.current=null;}},[value]);
 const insert=(tool:typeof tools[number])=>{
  const element=textarea.current;if(!element)return;
  const start=element.selectionStart,end=element.selectionEnd,chosen=value.slice(start,end)||tool.sample;
  const before=(tool.block&&start>0&&value[start-1]!=='\n'?'\n':'')+tool.before;
  const next=value.slice(0,start)+before+chosen+tool.after+value.slice(end);
  if(next.length>maxLength){setLimitError(true);return;}
  selection.current=[start+before.length,start+before.length+chosen.length];setLimitError(false);onChange(next);
 };
 return <div className="markdown-editor">
  <div className="markdown-editor-bar"><div className="markdown-modes" role="group" aria-label="Markdown 编辑模式">{(['edit','split','preview'] as const).map(m=><button key={m} type="button" aria-pressed={mode===m} onClick={()=>setMode(m)}>{m==='edit'?'编辑':m==='split'?'分栏':'预览'}</button>)}</div><span>Markdown</span></div>
  {mode!=='preview'&&<div className="markdown-tools" role="toolbar" aria-label="Markdown 格式">{tools.map(tool=><button type="button" key={tool.label} title={tool.label} aria-label={tool.label} disabled={disabled} onMouseDown={event=>event.preventDefault()} onClick={()=>insert(tool)}><tool.icon size={16}/></button>)}</div>}
  <div className={`markdown-editor-panes ${mode}`}>
   {mode!=='preview'&&<div className="markdown-source"><label className="sr-only" htmlFor={id}>内容（Markdown）</label><textarea id={id} ref={textarea} value={value} disabled={disabled} required maxLength={maxLength} spellCheck={false} placeholder="使用 Markdown 编写公告…" onChange={event=>{setLimitError(false);onChange(event.target.value);}}/></div>}
   {mode!=='edit'&&<div className="markdown-preview" role="region" aria-label="Markdown 预览">{value.trim()?<MarkdownContent>{value}</MarkdownContent>:<p className="markdown-preview-empty">预览将在这里显示</p>}</div>}
  </div>
  <div className="markdown-editor-footer"><span>{limitError?'内容已达到字数限制':'支持标题、列表、链接、图片、表格和代码块'}</span><span>{value.length} / {maxLength}</span></div>
 </div>;
}
