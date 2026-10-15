import {invoke} from '@tauri-apps/api/core';
import {native} from './bridge';
export type McpGrant={id:string;name:string;profileIds:string[];permissions:string[];origins:string[];expiresAt:number;confirmInteractions:boolean;fileRoots:string[];revoked:boolean};
export type McpJob={id:string;clientId:string;tool:string;arguments:Record<string,unknown>;state:string;target?:{title?:string;url?:string;name?:string;type?:string;message?:string};result:unknown;error:string|null;createdAt:number};
export type McpRemoteRequest={user_code:string;client_name:string;redirect_uri:string;scopes:string[];resource:string};
export type McpRemoteRoute={routeId:string;localGrantId:string;accountId:string;endpoint:string};
export type McpStatus={enabled:boolean;ready:boolean;error:string|null;grants:McpGrant[];jobs:McpJob[];remote?:{routes:McpRemoteRoute[];lastConnected:number;error:string|null;resource?:string;activeRoutes?:string[]}};
export const emptyMcpStatus:McpStatus={enabled:false,ready:false,error:null,grants:[],jobs:[]};
export async function mcpStatus():Promise<McpStatus>{return native?invoke('mcp_status'):emptyMcpStatus;}
export async function mcpAction(action:Record<string,unknown>):Promise<McpStatus>{if(!native)throw Error('请在 NodeCloak 桌面端配置 MCP');return invoke('mcp_action',{action});}
export async function mcpConfiguration(id:string):Promise<unknown>{return invoke('mcp_action',{action:{kind:'configuration',id}});}
export async function mcpRemoteRequest(code:string):Promise<McpRemoteRequest>{if(!native)throw Error('请在 NodeCloak 桌面端配置 MCP');return invoke('mcp_action',{action:{kind:'remote_request',user_code:code}});}
