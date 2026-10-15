import {invoke} from '@tauri-apps/api/core';
import {native} from './bridge';

export type TerminalTool = 'claude' | 'codex' | 'gemini' | 'shell';
export interface TerminalCatalog {
  homeDirectory: string;
  tools: {id: TerminalTool; installed: boolean}[];
}
export const terminalTools: {id: TerminalTool; name: string; brand: string; command: string; guide?: string}[] = [
  {id:'claude',name:'Claude Code',brand:'Anthropic',command:'claude',guide:'https://code.claude.com/docs/en/setup'},
  {id:'codex',name:'Codex CLI',brand:'ChatGPT · OpenAI',command:'codex',guide:'https://learn.chatgpt.com/docs/codex/cli'},
  {id:'gemini',name:'Gemini CLI',brand:'Google',command:'gemini',guide:'https://geminicli.com/docs/get-started/installation/'},
  {id:'shell',name:'普通终端',brand:'YOUR WORKSPACE',command:''},
];
export async function getTerminalCatalog(): Promise<TerminalCatalog> {
  if(native) return invoke('list_terminal_tools');
  return {homeDirectory:'',tools:terminalTools.map(t=>({id:t.id,installed:t.id!=='gemini'}))};
}
export async function launchTerminal(tool:TerminalTool,workingDirectory:string,environment:TerminalEnvironment=terminalEnvironmentDefaults()):Promise<void> {
  if(native) return invoke('launch_terminal',{tool,workingDirectory,environment});
  throw new Error('当前是界面演示。请在桌面版中打开终端。');
}
export async function selectTerminalDirectory(workingDirectory:string,title:string):Promise<string|null> {
  if(native) return invoke('select_terminal_directory',{workingDirectory,title});
  throw new Error('请在桌面版中选择工作目录。');
}
export async function openTerminalGuide(tool:TerminalTool):Promise<void> {
  if(native) return invoke('open_terminal_guide',{tool});
  const guide=terminalTools.find(t=>t.id===tool)?.guide;
  if(guide) window.open(guide,'_blank','noopener,noreferrer');
}

export interface TerminalEnvironment {mode:'inherit'|'custom';timezone:string;lang:string;lcAll:string;variables:{name:string;value:string}[]}
export const terminalEnvironmentDefaults=():TerminalEnvironment=>({mode:'custom',timezone:'Asia/Singapore',lang:'en_US.UTF-8',lcAll:'en_US.UTF-8',variables:[]});
const demoEnvironments=new Map<TerminalTool,TerminalEnvironment>();
export async function getTerminalEnvironment(tool:TerminalTool):Promise<TerminalEnvironment> {
  if(native)return invoke('get_terminal_environment',{tool});
  return structuredClone(demoEnvironments.get(tool)??terminalEnvironmentDefaults());
}
export async function saveTerminalEnvironment(tool:TerminalTool,environment:TerminalEnvironment):Promise<void> {
  if(native)return invoke('save_terminal_environment',{tool,environment});
  demoEnvironments.set(tool,structuredClone(environment));
}
export function validateTerminalEnvironment(environment:TerminalEnvironment):string|null {
  if(environment.variables.length>32||new TextEncoder().encode(environment.variables.map(v=>v.name+v.value).join('')).length>24576)return '终端环境配置过大，最多 32 个变量、共 24 KiB。';
  if(environment.mode==='inherit')return null;
  if(environment.timezone){try{new Intl.DateTimeFormat('en',{timeZone:environment.timezone});}catch{return '请填写有效的 IANA 时区，例如 Asia/Singapore。';}}
  if([environment.lang,environment.lcAll].some(v=>v&&!/^[A-Za-z0-9_.@-]+$/.test(v)))return '语言格式无效，例如 en_US.UTF-8。';
  const names=new Set<string>();
  for(const {name,value} of environment.variables){
    if(!name&&!value)continue;
    if(!/^[A-Za-z_][A-Za-z0-9_]{0,127}$/.test(name))return '变量名需以字母或下划线开头，仅包含字母、数字和下划线。';
    const upper=name.toUpperCase();
    if(upper.startsWith('NODECLOAK_')||['TZ','LANG','LC_ALL'].includes(upper))return 'TZ、LANG、LC_ALL 请使用上方字段；NODECLOAK_ 为保留前缀。';
    if(names.has(upper))return '环境变量名重复，请检查大小写。';names.add(upper);
    if(value.includes('\0'))return '环境变量值不能包含空字符。';
  }
  return null;
}
