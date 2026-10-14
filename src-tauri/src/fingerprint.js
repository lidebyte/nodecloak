function nodecloakFingerprint(config) {
  if (!config.enabled || config.version !== 1 || !/^[0-9a-f]{64}$/.test(config.seed)) return;
  const marker=Symbol.for('nodecloak.readback.v1');
  if(globalThis[marker])return;
  Object.defineProperty(globalThis,marker,{value:true});
  const hash=text=>{let h=2166136261;for(let i=0;i<text.length;i++)h=Math.imul(h^text.charCodeAt(i),16777619);h^=h>>>16;h=Math.imul(h,0x7feb352d);return (h^(h>>>15))>>>0;};
  // Firefox may invoke a Window preload before its final URL is committed.
  // Resolve the origin when an API is read, after website navigation begins.
  const key=tag=>hash(`${config.seed}:${globalThis.origin||globalThis.location?.origin||'opaque'}:${tag}`);
  const mix=n=>{n=Math.imul(n^(n>>>16),0x45d9f3b);return (n^(n>>>16))>>>0;};
  const wrap=(proto,name,handler)=>{if(!proto)return;const d=Object.getOwnPropertyDescriptor(proto,name);if(typeof d?.value!=='function'||!d.configurable)return;const native=d.value;Object.defineProperty(proto,name,{...d,value:function(...args){return handler.call(this,native,args);}});};
  const pixels=(data,width,height,x=0,y=0,offset=0,tag='pixels')=>{
    if(!['[object Uint8Array]','[object Uint8ClampedArray]','[object Float32Array]'].includes(Object.prototype.toString.call(data))||!Number.isSafeInteger(width)||!Number.isSafeInteger(height)||width<=0||height<=0||width*height>4194304)return;
    const start=key(tag);const length=Math.min(data.length-offset,width*height*4);
    for(let i=0;i<length;i+=4){if(data[offset+i+3]!== (data.BYTES_PER_ELEMENT===1?255:1))continue;const pixel=i/4,px=(x+pixel%width)|0,py=(y+Math.floor(pixel/width))|0;const h=mix(start^px^Math.imul(py,65537));if((px!==0||py!==0)&&(h&31)!==0)continue;
      for(let c=0;c<3;c++){const at=offset+i+c,v=data[at];if(!Number.isFinite(v))continue;const direction=(h>>c)&1?1:-1;data[at]=data.BYTES_PER_ELEMENT===1?v+(v===0?1:v===255?-1:direction):v+direction*1e-7;}
    }
  };
  const contexts=[globalThis.CanvasRenderingContext2D?.prototype,globalThis.OffscreenCanvasRenderingContext2D?.prototype].filter(Boolean);
  const nativeGet=contexts.map(p=>p.getImageData);
  if(config.canvasNoise)for(const p of contexts)wrap(p,'getImageData',function(native,args){if(args.length<4)return Reflect.apply(native,this,args);const numbers=args.slice(0,4).map(value=>Math.trunc(+value)),result=Reflect.apply(native,this,[...numbers,...args.slice(4)]);if(!result.colorSpace||result.colorSpace==='srgb')pixels(result.data,result.width,result.height,numbers[0]+(numbers[2]<0?-result.width:0),numbers[1]+(numbers[3]<0?-result.height:0));return result;});
  if(config.fontNoise){const widths=new WeakMap(),prototypes=new WeakSet();for(const p of contexts)wrap(p,'measureText',function(native,args){const result=Reflect.apply(native,this,args),proto=Object.getPrototypeOf(result),d=Object.getOwnPropertyDescriptor(proto,'width');if(d?.get&&d.configurable&&!prototypes.has(proto)){Object.defineProperty(proto,'width',{...d,get:function(){const width=Reflect.apply(d.get,this,[]);return width+(widths.get(this)||0);}});prototypes.add(proto);}if(result.width!==0)widths.set(result,((key('font')%2001)-1000||1)*1e-6);return result;});}
  // Export a noisy copy. The displayed/source canvas and caller pixels stay intact.
  const attributes=new WeakMap();
  if(config.canvasNoise)for(const proto of [globalThis.HTMLCanvasElement?.prototype,globalThis.OffscreenCanvas?.prototype])wrap(proto,'getContext',function(native,args){const context=Reflect.apply(native,this,args);if(context&&typeof context.getContextAttributes==='function')attributes.set(this,context.getContextAttributes());return context;});
  const canvasCopy=canvas=>{
    if(!canvas.width||!canvas.height||canvas.width*canvas.height>4194304)return canvas;
    const attr=attributes.get(canvas);if(attr&&(attr.colorSpace&&attr.colorSpace!=='srgb'||attr.colorType&&attr.colorType!=='unorm8'))return canvas;
    const offscreen=typeof globalThis.OffscreenCanvas==='function'&&canvas instanceof globalThis.OffscreenCanvas;
    const copy=offscreen?new OffscreenCanvas(canvas.width,canvas.height):canvas.ownerDocument.createElement('canvas');copy.width=canvas.width;copy.height=canvas.height;
    const ctx=copy.getContext('2d');if(!ctx)return canvas;ctx.drawImage(canvas,0,0);
    const get=nativeGet[offscreen?contexts.indexOf(globalThis.OffscreenCanvasRenderingContext2D?.prototype):contexts.indexOf(globalThis.CanvasRenderingContext2D?.prototype)];
    const image=Reflect.apply(get,ctx,[0,0,copy.width,copy.height]);pixels(image.data,image.width,image.height);ctx.putImageData(image,0,0);return copy;
  };
  if(config.canvasNoise){
    for(const name of ['toDataURL','toBlob'])wrap(globalThis.HTMLCanvasElement?.prototype,name,function(native,args){let copy;try{copy=canvasCopy(this);}catch{return Reflect.apply(native,this,args);}return Reflect.apply(native,copy,args);});
    wrap(globalThis.OffscreenCanvas?.prototype,'convertToBlob',function(native,args){let copy;try{copy=canvasCopy(this);}catch{return Reflect.apply(native,this,args);}return Reflect.apply(native,copy,args);});
  }
  if(config.webglNoise)for(const p of [globalThis.WebGLRenderingContext?.prototype,globalThis.WebGL2RenderingContext?.prototype])wrap(p,'readPixels',function(native,args){
    const result=Reflect.apply(native,this,args),[x,y,w,h,format,type,out]=args,offset=args[7]??0;
    const byte=type===5121&&['[object Uint8Array]','[object Uint8ClampedArray]'].includes(Object.prototype.toString.call(out)),float=type===5126&&Object.prototype.toString.call(out)==='[object Float32Array]';
    if(format!==6408||(!byte&&!float)||![x,y,w,h,offset].every(Number.isSafeInteger)||x<0||y<0||w<=0||h<=0||offset<0||out.length-offset<w*h*4||this.isContextLost?.())return result;
    // Restrict to complete, tightly packed reads of the default drawing buffer.
    // Never query getError: errors belong to the caller, and failed reads must stay untouched.
    const webgl2=!!globalThis.WebGL2RenderingContext&&this instanceof WebGL2RenderingContext;
    if(this.getParameter(webgl2?36010:36006)||x+w>this.drawingBufferWidth||y+h>this.drawingBufferHeight)return result;
    if(float&&(this.getParameter(35739)!==format||this.getParameter(35738)!==type))return result;
    const alignment=this.getParameter(3333),rowBytes=w*4*out.BYTES_PER_ELEMENT;
    if(Math.ceil(rowBytes/alignment)*alignment!==rowBytes)return result;
    if(webgl2&&(this.getParameter(35053)||this.getParameter(3330)||this.getParameter(3331)||this.getParameter(3332)))return result;
    pixels(out,w,h,x,y,offset,'webgl');return result;
  });
  const audioNoise=(data,channel=0,limit=data?.length,scale=1)=>{if(!ArrayBuffer.isView(data)||data.length>4194304)return;const base=key('audio')^channel;for(let i=0;i<Math.min(data.length,limit);i+=16){if(!Number.isFinite(data[i]))continue;const h=mix(base^i);data[i]+=((h&1)?1:-1)*((h%7)+1)*1e-8*scale;}};
  if(config.audioNoise){
    const rendered=new WeakSet();
    const protect=buffer=>{if(rendered.has(buffer))return;rendered.add(buffer);for(let i=0;i<buffer.numberOfChannels;i++)audioNoise(buffer.getChannelData(i),i);};
    for(const C of [globalThis.OfflineAudioContext,globalThis.webkitOfflineAudioContext])wrap(C?.prototype,'startRendering',function(native,args){this.addEventListener('complete',e=>protect(e.renderedBuffer),{capture:true,once:true});const result=Reflect.apply(native,this,args);return result?.then?result.then(buffer=>{protect(buffer);return buffer;}):result;});
    for(const name of ['getFloatFrequencyData','getFloatTimeDomainData'])wrap(globalThis.AnalyserNode?.prototype,name,function(native,args){const result=Reflect.apply(native,this,args);audioNoise(args[0],0,name==='getFloatFrequencyData'?this.frequencyBinCount:this.fftSize,name==='getFloatFrequencyData'?1000:1);return result;});
  }
  if(config.mediaNoise){
    const reverse=new Map();
    const alias=(id,tag)=>{if(typeof id!=='string'||!id||id==='default'||id==='communications')return id;const value=[0,1,2,3].map(i=>key(`media:${tag}:${i}:${id}`).toString(16).padStart(8,'0')).join('');if(reverse.size>4096)reverse.clear();reverse.set(`${tag}:${value}`,id);return value;};
    const info=globalThis.MediaDeviceInfo?.prototype;
    const kind=info&&Object.getOwnPropertyDescriptor(info,'kind')?.get;
    // Output routing APIs require real sink IDs and transient user activation.
    // Preserve output identities rather than delaying or breaking speaker selection.
    for(const [field,tag] of [['deviceId','device'],['groupId','group']]){const d=info&&Object.getOwnPropertyDescriptor(info,field);if(d?.get&&d.configurable)Object.defineProperty(info,field,{...d,get:function(){const value=Reflect.apply(d.get,this,[]);return kind&&Reflect.apply(kind,this,[])==='audiooutput'?value:alias(value,tag);}});}
    wrap(info,'toJSON',function(native,args){const result=Reflect.apply(native,this,args);return result.kind==='audiooutput'?result:{...result,deviceId:alias(result.deviceId,'device'),groupId:alias(result.groupId,'group')};});
    const id=(value,tag,encode)=>typeof value==='string'?(encode?alias(value,tag):reverse.get(`${tag}:${value}`)||value):Array.isArray(value)?value.map(v=>id(v,tag,encode)):value;
    const constraint=(value,encode=false,depth=0)=>{if(!value||typeof value!=='object'||depth>8)return value;const result={...value};for(const [field,tag] of [['deviceId','device'],['groupId','group']]){const current=result[field];if(current===undefined)continue;result[field]=!Array.isArray(current)&&typeof current==='object'&&current!==null?{...current,...('exact'in current?{exact:id(current.exact,tag,encode)}:{}),...('ideal'in current?{ideal:id(current.ideal,tag,encode)}:{})}:id(current,tag,encode);}if(Array.isArray(result.advanced))result.advanced=result.advanced.map(v=>constraint(v,encode,depth+1));return result;};
    const devices=globalThis.MediaDevices?.prototype,enumerate=devices?.enumerateDevices;
    const fields=['width','height','aspectRatio','frameRate','facingMode','resizeMode','sampleRate','sampleSize','echoCancellation','autoGainControl','noiseSuppression','latency','channelCount','deviceId','groupId','displaySurface','logicalSurface','cursor','restrictOwnAudio','suppressLocalAudioPlayback','pan','tilt','zoom','torch','backgroundBlur'];
    const snapshotValue=value=>{if(Array.isArray(value))return value.map(snapshotValue);if(value&&typeof value==='object'){const result={};for(const field of ['exact','ideal','min','max']){const item=value[field];if(item!==undefined)result[field]=Array.isArray(item)?item.slice():item;}return result;}return value;};
    const snapshot=(value,keys,depth=0)=>{if(!value||typeof value!=='object'||depth>8)return value;const result={};for(const field of keys){const item=value[field];if(item!==undefined)result[field]=snapshotValue(item);}const advanced=value.advanced;if(Array.isArray(advanced))result.advanced=advanced.map(item=>snapshot(item,keys,depth+1));return result;};
    const unresolved=value=>{if(typeof value==='string')return /^[0-9a-f]{32}$/.test(value)&&!reverse.has(`device:${value}`)&&!reverse.has(`group:${value}`);if(Array.isArray(value))return value.some(unresolved);return !!value&&typeof value==='object'&&Object.values(value).some(unresolved);};
    wrap(devices,'getUserMedia',function(native,args){const value=args[0];if(!value||typeof value!=='object')return Reflect.apply(native,this,args);let keys=fields;if(typeof this.getSupportedConstraints==='function')keys=[...new Set([...fields,...Object.keys(this.getSupportedConstraints())])];const saved={video:snapshot(value.video,keys),audio:snapshot(value.audio,keys)};const select=()=>Reflect.apply(native,this,[{video:constraint(saved.video),audio:constraint(saved.audio)}]);if(!enumerate||!unresolved(saved))return select();return Reflect.apply(enumerate,this,[]).then(list=>{for(const device of list){void device.deviceId;void device.groupId;}return select();},()=>select());});
    for(const name of ['getSettings','getCapabilities'])wrap(globalThis.MediaStreamTrack?.prototype,name,function(native,args){const result=Reflect.apply(native,this,args);return {...result,...('deviceId'in result?{deviceId:alias(result.deviceId,'device')}:{}),...('groupId'in result?{groupId:alias(result.groupId,'group')}:{})};});
    wrap(globalThis.MediaStreamTrack?.prototype,'applyConstraints',function(native,args){return Reflect.apply(native,this,[constraint(snapshot(args[0],fields))]);});
    wrap(globalThis.MediaStreamTrack?.prototype,'getConstraints',function(native,args){return constraint(Reflect.apply(native,this,args),true);});
  }
}
