// Runs only in a synthetic, network-intercepted page of an isolated managed profile.
// No executable source or constructors are accepted from synchronized data.
(() => {
 const MAX_NODES=50000,MAX_STRING=64<<20,MAX_BYTES=64<<20,MAX_RECORDS=50000;
 const typed={Uint8Array,Uint8ClampedArray,Int8Array,Int16Array,Uint16Array,Int32Array,Uint32Array,Float32Array,Float64Array,BigInt64Array,BigUint64Array,DataView};
 const fail=message=>{throw new Error(message);};
 const bytesToBase64=bytes=>{let text='';for(let i=0;i<bytes.length;i+=8192)text+=String.fromCharCode(...bytes.subarray(i,i+8192));return btoa(text);};
 const base64ToBytes=text=>{if(typeof text!=='string'||text.length>2*MAX_STRING)fail('NC_LIMIT');return Uint8Array.from(atob(text),x=>x.charCodeAt(0));};
 const string=s=>{if(typeof s!=='string'||s.length>MAX_STRING)fail('NC_LIMIT');return s;};
 const checkBytes=state=>{if(new Blob([JSON.stringify(state)]).size>MAX_BYTES)fail('NC_LIMIT');};
 async function pack(value){
  const nodes=[],seen=new WeakMap();
  async function encode(v,depth=0){
   if(depth>100)fail('NC_LIMIT');if(v===null||typeof v==='boolean')return v;if(typeof v==='string')return string(v);
   if(typeof v==='number')return Number.isFinite(v)&&!Object.is(v,-0)?v:{number:Object.is(v,-0)?'-0':String(v)};
   if(typeof v==='undefined')return {undefined:true};if(typeof v==='bigint')return {bigint:string(String(v))};if(typeof v!=='object')fail('NC_UNSUPPORTED_VALUE');
   if(seen.has(v))return {ref:seen.get(v)};if(nodes.length>=MAX_NODES)fail('NC_LIMIT');const id=nodes.length;seen.set(v,id);const node={};nodes.push(node);
   if(Array.isArray(v)){if(v.length>MAX_RECORDS)fail('NC_LIMIT');node.type='Array';node.values=[];for(const x of v)node.values.push(await encode(x,depth+1));}
   else if(v instanceof Date){node.type='Date';node.value=Number.isNaN(v.getTime())?null:v.getTime();}
   else if(v instanceof RegExp){node.type='RegExp';node.source=string(v.source);node.flags=v.flags;}
   else if(v instanceof Map){if(v.size>MAX_RECORDS)fail('NC_LIMIT');node.type='Map';node.values=[];for(const[k,x]of v)node.values.push([await encode(k,depth+1),await encode(x,depth+1)]);}
   else if(v instanceof Set){if(v.size>MAX_RECORDS)fail('NC_LIMIT');node.type='Set';node.values=[];for(const x of v)node.values.push(await encode(x,depth+1));}
   else if(v instanceof ArrayBuffer){if(v.byteLength>MAX_STRING)fail('NC_LIMIT');node.type='ArrayBuffer';node.data=bytesToBase64(new Uint8Array(v));}
   else if(ArrayBuffer.isView(v)){if(v.byteLength>MAX_STRING)fail('NC_LIMIT');node.type='TypedArray';node.name=v.constructor.name;if(!Object.hasOwn(typed,node.name))fail('NC_UNSUPPORTED_VALUE');node.data=bytesToBase64(new Uint8Array(v.buffer,v.byteOffset,v.byteLength));}
   else if(v instanceof Blob){if(v.size>MAX_STRING)fail('NC_LIMIT');node.type=typeof File!=='undefined'&&v instanceof File?'File':'Blob';node.mime=string(v.type);node.data=bytesToBase64(new Uint8Array(await v.arrayBuffer()));if(node.type==='File'){node.name=string(v.name);node.lastModified=v.lastModified;}}
   else if(Object.prototype.toString.call(v)==='[object CryptoKey]'){
    if(!v.extractable)fail('non-exportable CryptoKey');node.type='CryptoKey';node.algorithm=v.algorithm;node.usages=v.usages;node.jwk=await crypto.subtle.exportKey('jwk',v);
   }else if(Object.getPrototypeOf(v)===Object.prototype||Object.getPrototypeOf(v)===null){node.type='Object';node.values=[];const entries=Object.entries(v);if(entries.length>MAX_RECORDS)fail('NC_LIMIT');for(const[k,x]of entries)node.values.push([string(k),await encode(x,depth+1)]);}
   else fail('NC_UNSUPPORTED_VALUE');return {ref:id};
  }
  return {root:await encode(value),nodes};
 }
 async function unpack(graph){
  if(!graph||!Array.isArray(graph.nodes)||graph.nodes.length>MAX_NODES)fail('NC_INVALID_GRAPH');const cache=new Map();
  const list=v=>{if(!Array.isArray(v)||v.length>MAX_RECORDS)fail('NC_LIMIT');return v;};
  async function decode(v,depth=0){
   if(depth>100)fail('NC_LIMIT');if(v===null||typeof v==='boolean')return v;if(typeof v==='string')return string(v);if(typeof v==='number'&&Number.isFinite(v))return v;
   if(!v||typeof v!=='object'||Array.isArray(v))fail('NC_INVALID_GRAPH');
   if(Object.hasOwn(v,'undefined')&&v.undefined===true)return undefined;
   if(Object.hasOwn(v,'bigint')){if(!/^-?\d+$/.test(string(v.bigint)))fail('NC_INVALID_GRAPH');return BigInt(v.bigint);}
   if(Object.hasOwn(v,'number')){if(!['NaN','Infinity','-Infinity','-0'].includes(v.number))fail('NC_INVALID_GRAPH');return v.number==='-0'?-0:Number(v.number);}
   const id=v.ref;if(!Number.isInteger(id)||id<0||id>=graph.nodes.length)fail('NC_INVALID_GRAPH');if(cache.has(id))return cache.get(id);
   const n=graph.nodes[id];let result;if(!n||typeof n!=='object')fail('NC_INVALID_GRAPH');
   switch(n.type){
    case 'Object':result={};break;case 'Array':result=[];break;case 'Map':result=new Map();break;case 'Set':result=new Set();break;
    case 'Date':if(n.value!==null&&!Number.isFinite(n.value))fail('NC_INVALID_GRAPH');result=new Date(n.value===null?NaN:n.value);break;
    case 'RegExp':result=new RegExp(string(n.source),string(n.flags));break;
    case 'ArrayBuffer':result=base64ToBytes(n.data).buffer;break;
    case 'TypedArray':{if(!Object.hasOwn(typed,n.name))fail('NC_INVALID_GRAPH');const buffer=base64ToBytes(n.data).buffer;result=new typed[n.name](buffer);break;}
    case 'Blob':result=new Blob([base64ToBytes(n.data)],{type:string(n.mime)});break;
    case 'File':if(!Number.isFinite(n.lastModified))fail('NC_INVALID_GRAPH');result=new File([base64ToBytes(n.data)],string(n.name),{type:string(n.mime),lastModified:n.lastModified});break;
    case 'CryptoKey':{const allowed=new Set(['AES-GCM','AES-CBC','AES-CTR','AES-KW','HMAC','RSA-OAEP','RSA-PSS','RSASSA-PKCS1-v1_5','ECDSA','ECDH','Ed25519','X25519']);if(!n.algorithm||!allowed.has(n.algorithm.name)||!Array.isArray(n.usages)||n.usages.length>8)fail('NC_INVALID_GRAPH');result=await crypto.subtle.importKey('jwk',n.jwk,n.algorithm,true,n.usages);break;}
    default:fail('NC_INVALID_GRAPH');
   }
   cache.set(id,result);
   if(n.type==='Object'){for(const pair of list(n.values)){if(!Array.isArray(pair)||pair.length!==2)fail('NC_INVALID_GRAPH');Object.defineProperty(result,string(pair[0]),{value:await decode(pair[1],depth+1),writable:true,enumerable:true,configurable:true});}}
   if(n.type==='Array'){for(const x of list(n.values))result.push(await decode(x,depth+1));}
   if(n.type==='Map'){for(const pair of list(n.values)){if(!Array.isArray(pair)||pair.length!==2)fail('NC_INVALID_GRAPH');result.set(await decode(pair[0],depth+1),await decode(pair[1],depth+1));}}
   if(n.type==='Set'){for(const x of list(n.values))result.add(await decode(x,depth+1));}
   return result;
  }
  return decode(graph.root);
 }
 const open=(name,version,upgrade)=>new Promise((resolve,reject)=>{const r=version===undefined?indexedDB.open(name):indexedDB.open(name,version);r.onupgradeneeded=()=>{try{upgrade?.(r.result);}catch(e){r.transaction.abort();reject(e);}};r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(new Error('NC_STORAGE_BLOCKED'));r.onblocked=()=>reject(new Error('NC_STORAGE_BLOCKED'));});
 const deleted=name=>new Promise((resolve,reject)=>{const r=indexedDB.deleteDatabase(name);r.onsuccess=()=>resolve();r.onerror=r.onblocked=()=>reject(new Error('NC_STORAGE_BLOCKED'));});
 async function exportOrigin(){
  const local=Object.entries(localStorage).map(([key,value])=>[string(key),string(value)]),databases=[];if(local.length>MAX_RECORDS)fail('NC_LIMIT');const names=await indexedDB.databases();if(names.length>100)fail('NC_LIMIT');
  for(const entry of names){if(!entry.name)continue;const db=await open(entry.name);try{
   const stores=[],storeNames=[...db.objectStoreNames];if(storeNames.length>100)fail('NC_LIMIT');
   const records=await new Promise((resolve,reject)=>{if(!storeNames.length){resolve([]);return;}const tx=db.transaction(storeNames,'readonly');const all=[];let count=0;
    for(const name of storeNames){const store=tx.objectStore(name),rows=[];all.push({name,keyPath:store.keyPath,autoIncrement:store.autoIncrement,indexes:[...store.indexNames].map(n=>{const i=store.index(n);return {name:n,keyPath:i.keyPath,unique:i.unique,multiEntry:i.multiEntry};}),rows});const cursor=store.openCursor();cursor.onsuccess=()=>{const c=cursor.result;if(!c)return;if(++count>MAX_RECORDS){tx.abort();return;}rows.push([c.primaryKey,c.value]);c.continue();};cursor.onerror=()=>tx.abort();}
    tx.oncomplete=()=>resolve(all);tx.onabort=tx.onerror=()=>reject(new Error('NC_STORAGE_BLOCKED'));
   });
   for(const store of records){const encoded=[];for(const[k,v]of store.rows)encoded.push([await pack(k),await pack(v)]);stores.push({...store,rows:encoded});}
   databases.push({name:entry.name,version:db.version,stores});
  }finally{db.close();}}
  const state={local,databases};checkBytes(state);return state;
 }
 async function importOrigin(state){
  if(!state||!Array.isArray(state.local)||state.local.length>MAX_RECORDS||!Array.isArray(state.databases)||state.databases.length>100)fail('NC_INVALID_GRAPH');
  checkBytes(state);
  const databases=[];for(const d of state.databases){if(!d||typeof d.name!=='string'||d.name.length>1000||!Number.isSafeInteger(d.version)||d.version<1||!Array.isArray(d.stores)||d.stores.length>100)fail('NC_INVALID_GRAPH');const stores=[];for(const s of d.stores){if(!s||typeof s.name!=='string'||s.name.length>1000||!Array.isArray(s.rows)||s.rows.length>MAX_RECORDS||!Array.isArray(s.indexes)||s.indexes.length>100||typeof s.autoIncrement!=='boolean')fail('NC_INVALID_GRAPH');const rows=[];for(const pair of s.rows){if(!Array.isArray(pair)||pair.length!==2)fail('NC_INVALID_GRAPH');rows.push([await unpack(pair[0]),await unpack(pair[1])]);}stores.push({...s,rows});}databases.push({...d,stores});}
  const local=state.local.map(pair=>{if(!Array.isArray(pair)||pair.length!==2)fail('NC_INVALID_GRAPH');return [string(pair[0]),string(pair[1])];});
  for(const d of await indexedDB.databases()){if(d.name)await deleted(d.name);}localStorage.clear();for(const[k,v]of local)localStorage.setItem(k,v);
  for(const d of databases){const db=await open(d.name,d.version,db=>{for(const s of d.stores){const store=db.createObjectStore(s.name,{keyPath:s.keyPath,autoIncrement:s.autoIncrement});for(const i of s.indexes){if(!i||typeof i.name!=='string'||typeof i.unique!=='boolean'||typeof i.multiEntry!=='boolean')fail('NC_INVALID_GRAPH');store.createIndex(i.name,i.keyPath,{unique:i.unique,multiEntry:i.multiEntry});}}});try{
   if(d.stores.length)await new Promise((resolve,reject)=>{const tx=db.transaction(d.stores.map(s=>s.name),'readwrite');tx.oncomplete=()=>resolve();tx.onabort=tx.onerror=()=>reject(new Error('NC_STORAGE_BLOCKED'));try{for(const s of d.stores){const store=tx.objectStore(s.name);for(const[k,v]of s.rows){if(s.keyPath===null)store.put(v,k);else store.put(v);}}}catch(e){tx.abort();reject(new Error('NC_STORAGE_BLOCKED'));}});
  }finally{db.close();}}
  return true;
 }
 globalThis.__ncSessionCodec={pack,unpack,exportOrigin,importOrigin};
})();
