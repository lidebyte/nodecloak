// @vitest-environment jsdom
import {act} from 'react';
import {createRoot,type Root} from 'react-dom/client';
import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import TerminalWorkspace from './TerminalWorkspace';
import {terminalEnvironmentDefaults,validateTerminalEnvironment} from './terminal';
import {setLanguage} from './i18n';
const api=vi.hoisted(()=>({load:vi.fn(),save:vi.fn(),launch:vi.fn()}));
vi.mock('./bridge',()=>({native:false}));
vi.mock('./terminal',async original=>({...await original<object>(),getTerminalCatalog:async()=>({homeDirectory:'C:\\Users\\test',tools:['claude','codex','gemini','shell'].map(id=>({id,installed:true}))}),getTerminalEnvironment:api.load,saveTerminalEnvironment:api.save,launchTerminal:api.launch}));
let host:HTMLDivElement,root:Root;
const click=async(label:string)=>{const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===label);expect(button).toBeTruthy();await act(async()=>button!.click());};
const input=async(label:string,value:string)=>{const field=host.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;expect(field).toBeTruthy();await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(field,value);field.dispatchEvent(new Event('input',{bubbles:true}));});};
beforeEach(async()=>{vi.stubGlobal('ResizeObserver',class{observe(){} disconnect(){} unobserve(){}});HTMLElement.prototype.scrollIntoView=vi.fn();setLanguage('zh');localStorage.clear();Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});api.load.mockImplementation(async()=>terminalEnvironmentDefaults());api.save.mockResolvedValue(undefined);api.launch.mockResolvedValue(undefined);host=document.createElement('div');document.body.append(host);root=createRoot(host);await act(async()=>root.render(<TerminalWorkspace busy={false} onBusyChange={()=>{}} onNotify={()=>{}}/>));});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();vi.resetAllMocks();vi.unstubAllGlobals();});
it('saves and launches the same per-tool configuration while masking variable values',async()=>{
 await input('时区','Europe/Paris');await input('语言（LANG）','zh_CN.UTF-8');await input('区域格式（LC_ALL）','');
 await click('添加环境变量');await input('变量名 1','OPENAI_API_KEY');await input('变量值 1','synthetic-private-value');
 expect(host.querySelector<HTMLInputElement>('[aria-label="变量值 1"]')!.type).toBe('password');
 const preview=host.querySelector('[aria-label="启动环境预览"]')!;expect(preview.textContent).toContain('Europe/Paris');expect(preview.textContent).toContain('OPENAI_API_KEY');expect(preview.textContent).not.toContain('synthetic-private-value');
 await click('保存环境');await click('打开Claude Code');
 const expected={mode:'custom',timezone:'Europe/Paris',lang:'zh_CN.UTF-8',lcAll:'',variables:[{name:'OPENAI_API_KEY',value:'synthetic-private-value'}]};
 expect(api.save).toHaveBeenCalledWith('claude',expected);expect(api.launch).toHaveBeenCalledWith('claude','',expected);expect(JSON.stringify(localStorage)).not.toContain('synthetic-private-value');
 await act(async()=>[...host.querySelectorAll<HTMLButtonElement>('.terminal-tool')].find(b=>b.querySelector('strong')?.textContent==='Codex CLI')!.click());
 expect(host.querySelector<HTMLInputElement>('[aria-label="时区"]')!.value).toBe('Asia/Singapore');
 await act(async()=>host.querySelector<HTMLButtonElement>('.terminal-tool')!.click());expect(host.querySelector<HTMLInputElement>('[aria-label="时区"]')!.value).toBe('Europe/Paris');
});
it('supports inheritance without a misleading fixed-region preview',async()=>{
 await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label="环境模式"]')!.click());
 await click('继承系统环境');
 expect(host.querySelector('[aria-label="时区"]')).toBeNull();
 expect(host.querySelector('[aria-label="启动环境预览"]')!.textContent).not.toContain('Asia/Singapore');
 await click('打开Claude Code');expect(api.launch.mock.calls[0][2].mode).toBe('inherit');
});
it('blocks duplicate and reserved names without exposing their values',()=>{
 const environment=terminalEnvironmentDefaults();environment.variables=[{name:'KEY',value:'hidden-a'},{name:'key',value:'hidden-b'}];expect(validateTerminalEnvironment(environment)).toContain('重复');
 environment.variables=[{name:'NODECLOAK_TERMINAL_DIRECTORY',value:'hidden-a'}];expect(validateTerminalEnvironment(environment)).toContain('保留前缀');
 environment.variables=[{name:'1INVALID',value:'hidden-a'}];expect(validateTerminalEnvironment(environment)).not.toContain('hidden-a');
});
