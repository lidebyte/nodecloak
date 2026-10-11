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
export async function launchTerminal(tool:TerminalTool,workingDirectory:string):Promise<void> {
  if(native) return invoke('launch_terminal',{tool,workingDirectory});
  throw new Error('当前是界面演示。请在桌面版中打开终端。');
}
export async function openTerminalGuide(tool:TerminalTool):Promise<void> {
  if(native) return invoke('open_terminal_guide',{tool});
  const guide=terminalTools.find(t=>t.id===tool)?.guide;
  if(guide) window.open(guide,'_blank','noopener,noreferrer');
}
