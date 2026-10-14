import {invoke} from '@tauri-apps/api/core';
import {native} from './bridge';
export interface FingerprintSettings {version:1;seed:string;enabled:boolean;canvasNoise:boolean;fontNoise:boolean;webglNoise:boolean;audioNoise:boolean;mediaNoise:boolean}
export function fingerprintSeed():string {return [...crypto.getRandomValues(new Uint8Array(32))].map(b=>b.toString(16).padStart(2,'0')).join('');}
export function fingerprintDefaults():FingerprintSettings {return {version:1,seed:fingerprintSeed(),enabled:true,canvasNoise:true,fontNoise:true,webglNoise:true,audioNoise:true,mediaNoise:true};}
export async function regenerateFingerprintSeed():Promise<string> {return native?invoke('generate_fingerprint_seed'):fingerprintSeed();}
