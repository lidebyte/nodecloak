import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const source=readFileSync(new URL('../src-tauri/src/fingerprint.js',import.meta.url),'utf8');
function realm(seed,enabled=true) {
 const realm=vm.createContext({seed,enabled});
 vm.runInContext(`
 globalThis.location={origin:'https://example.test'};
 class TextMetrics {get width(){return 123.5;}}
 class CanvasRenderingContext2D {getImageData(x=0,y=0,w=2,h=1){const converted=[x,y,w,h].map(value=>Math.trunc(+value));if(converted.some(value=>!Number.isFinite(value)||value<-2147483648||value>2147483647))throw new TypeError('long out of range');[x,y,w,h]=converted;w=Math.abs(w);h=Math.abs(h);const data=new Uint8ClampedArray(w*h*4);for(let i=0;i<data.length;i+=4)data.set([24,40,60,255],i);return {data,width:w,height:h};}measureText(){return new TextMetrics();}}
 class WebGLRenderingContext {constructor(){this.drawingBufferWidth=64;this.drawingBufferHeight=32;}getParameter(p){return p===3333?(this.alignment||4):p===35739?6408:p===35738?5121:null;}getError(){const value=this.error||0;this.error=0;return value;}readPixels(x,y,w,h,format,type,out,offset=0){if(type!==5121){this.error=1282;return;}for(let i=offset;i<offset+w*h*4;i+=4)out.set([64,64,64,255],i);}}
 class AudioBuffer {constructor(){this.length=512;this.numberOfChannels=1;this.data=new Float32Array(512).fill(.125);}getChannelData(){return this.data;}copyFromChannel(out){out.set(this.data.subarray(0,out.length));}}
 class OfflineAudioContext {constructor(){this.handlers=[];}addEventListener(name,handler){this.handlers.push(handler);}startRendering(){const renderedBuffer=new AudioBuffer();for(const fn of this.handlers)fn({renderedBuffer});return Promise.resolve(renderedBuffer);}}
 class MediaDeviceInfo {constructor(id,kind='videoinput'){this.id=id;this.deviceKind=kind;}get deviceId(){return this.id;}get groupId(){return 'physical-group';}get kind(){return this.deviceKind;}get label(){return 'Camera';}toJSON(){return {deviceId:this.id,groupId:'physical-group',kind:this.deviceKind,label:'Camera'};}}
 class MediaStreamTrack {applyConstraints(value){return Promise.resolve(value);}}
 class AnalyserNode {get frequencyBinCount(){return 16;}get fftSize(){return 32;}getFloatFrequencyData(out){out.fill(-30,0,Math.min(out.length,16));}getFloatTimeDomainData(out){out.fill(.125,0,Math.min(out.length,32));}}
class MediaDevices {enumerateDevices(){return Promise.resolve([new MediaDeviceInfo('camera-real'),new MediaDeviceInfo('')]);}getUserMedia(c){return Promise.resolve(c);}}
 Object.assign(globalThis,{CanvasRenderingContext2D,WebGLRenderingContext,OfflineAudioContext,AudioBuffer,MediaDeviceInfo,MediaDevices,MediaStreamTrack,AnalyserNode});
 `,realm);
 vm.runInContext(`(${source})({version:1,seed,enabled,canvasNoise:true,fontNoise:true,webglNoise:true,audioNoise:true,mediaNoise:true})`,realm);
 return {run:code=>vm.runInContext(code,realm)};
}
const seedA='11'.repeat(32),seedB='aa'.repeat(32);
test('canvas and text readback are stable, isolated and preserve alpha',()=>{
 const a=realm(seedA),b=realm(seedB),plain=realm(seedA,false);
 const pixels=r=>Array.from(r.run('new CanvasRenderingContext2D().getImageData(0,0,2,1).data'));
 assert.deepEqual(pixels(a),pixels(a));assert.notDeepEqual(pixels(a),pixels(plain));
 assert.notDeepEqual(pixels(a),pixels(b));assert.equal(pixels(a)[3],255);
 const width=r=>r.run('new CanvasRenderingContext2D().measureText("hello").width');
 assert.equal(width(a),width(a));assert.notEqual(width(a),width(plain));assert.notEqual(width(a),width(b));
});
test('webgl only changes the requested destination range and repeats consistently',()=>{
 const a=realm(seedA),plain=realm(seedA,false);
 const read=r=>Array.from(r.run('(()=>{const data=new Uint8Array(12).fill(9);new WebGLRenderingContext().readPixels(0,0,1,1,6408,5121,data,4);return data;})()'));
 assert.deepEqual(read(a),read(a));assert.notDeepEqual(read(a),read(plain));assert.deepEqual(read(a).slice(0,4),[9,9,9,9]);assert.deepEqual(read(a).slice(8),[9,9,9,9]);
});
test('offline audio noise is not cumulative and normal playback buffers stay native',async()=>{
 const a=realm(seedA),plain=realm(seedA,false);
 const buffer=await a.run('new OfflineAudioContext().startRendering()');
 const native=await plain.run('new OfflineAudioContext().startRendering()');
 assert.notDeepEqual(Array.from(buffer.getChannelData(0)),Array.from(native.getChannelData(0)));
 assert.deepEqual(Array.from(buffer.getChannelData(0)),Array.from(buffer.getChannelData(0)));
 assert.equal(a.run('new AudioBuffer().getChannelData(0)[0]'),.125);
});
test('media aliases preserve permission-hidden IDs, native objects and exact camera selection',async()=>{
 const a=realm(seedA),b=realm(seedB);
 const devices=await a.run('new MediaDevices().enumerateDevices()');
 const other=await b.run('new MediaDevices().enumerateDevices()');
 assert.notEqual(devices[0].deviceId,'camera-real');assert.notEqual(devices[0].deviceId,other[0].deviceId);assert.equal(devices[1].deviceId,'');
 assert.equal(devices[0].label,'Camera');assert.equal(devices[0].toJSON().deviceId,devices[0].deviceId);
 a.run('globalThis.savedDevices=new MediaDevices()');
 const selected=await a.run(`savedDevices.getUserMedia({video:{deviceId:{exact:${JSON.stringify(devices[0].deviceId)}}}})`);
 assert.equal(selected.video.deviceId.exact,'camera-real');
});
test('cropped and negative-dimension canvas reads use the same absolute pixel noise',()=>{
 const a=realm(seedA);
 const full=Array.from(a.run('new CanvasRenderingContext2D().getImageData(0,0,64,1).data'));
 for(let x=0;x<64;x++)assert.deepEqual(Array.from(a.run(`new CanvasRenderingContext2D().getImageData(${x},0,1,1).data`)),full.slice(x*4,x*4+4));
 assert.deepEqual(Array.from(a.run('new CanvasRenderingContext2D().getImageData(64,0,-64,1).data')),full);
});
test('text metrics retain native identity and borrowed getters',()=>{
 const a=realm(seedA);
 assert.equal(a.run('(()=>{const metrics=new CanvasRenderingContext2D().measureText("hello");return Object.getOwnPropertyDescriptor(Object.getPrototypeOf(metrics),"width").get.call(metrics)===metrics.width;})()'),true);
});
test('webgl preserves pending errors instead of consuming them',()=>{
 const a=realm(seedA);
 assert.equal(a.run('(()=>{const gl=new WebGLRenderingContext();gl.error=1280;gl.readPixels(0,0,1,1,6408,5121,new Uint8Array(4));return gl.getError();})()'),1280);
});
test('analyser noise affects real frequency samples and preserves untouched tails',()=>{
 const a=realm(seedA);
 const read=()=>Array.from(a.run('(()=>{const out=new Float32Array(64);new AnalyserNode().getFloatFrequencyData(out);return out;})()'));
 assert.notEqual(read()[0],-30);assert.deepEqual(read().slice(16),new Array(48).fill(0));assert.deepEqual(read(),read());
});
test('persisted device aliases are resolved after reload and in advanced constraints',async()=>{
 const devices=await realm(seedA).run('new MediaDevices().enumerateDevices()');
 const saved=devices[0].deviceId;
 const a=realm(seedA);
 const result=await a.run(`new MediaDevices().getUserMedia({video:{advanced:[{deviceId:{exact:${JSON.stringify(saved)}}}]}})`);
 assert.equal(result.video.advanced[0].deviceId.exact,'camera-real');
});
test('numeric canvas arguments are coerced once and share absolute coordinates',()=>{
 const a=realm(seedA);
 for(let x=0;x<64;x++)assert.deepEqual(Array.from(a.run(`new CanvasRenderingContext2D().getImageData("${x}","0","1","1").data`)),Array.from(a.run(`new CanvasRenderingContext2D().getImageData(${x},0,1,1).data`)));
 assert.equal(a.run('(()=>{let calls=0;new CanvasRenderingContext2D().getImageData({valueOf(){calls++;return 10;}},0,1,1);return calls;})()'),1);
});
test('nonopaque pixels retain native values',()=>{
 // Exercise the production noise through a native-like source with transparent output.
 const context=vm.createContext({});
 vm.runInContext(`globalThis.location={origin:'https://example.test'};class CanvasRenderingContext2D {getImageData(){return {width:1,height:1,data:new Uint8ClampedArray([0,0,0,0])};}}globalThis.CanvasRenderingContext2D=CanvasRenderingContext2D;(${source})({version:1,seed:'${seedA}',enabled:true,canvasNoise:true});`,context);
 assert.deepEqual(Array.from(vm.runInContext('new CanvasRenderingContext2D().getImageData(0,0,1,1).data',context)),[0,0,0,0]);
});
test('canvas invalid ranges remain native errors',()=>{
 const a=realm(seedA);
 for(const value of ['Infinity','NaN','2147483648','-2147483649'])assert.throws(()=>a.run(`new CanvasRenderingContext2D().getImageData(${value},0,1,1)`),{name:'TypeError'});
});
test('speaker IDs retain native routing and track constraints inherit dictionary fields',async()=>{
 const a=realm(seedA);
 assert.equal(a.run('new MediaDeviceInfo("speaker-real","audiooutput").deviceId'),'speaker-real');
 assert.equal(a.run('new MediaDeviceInfo("speaker-real","audiooutput").toJSON().deviceId'),'speaker-real');
 const result=await a.run('new MediaStreamTrack().applyConstraints(Object.create({width:640}))');
 assert.equal(result.width,640);
});
test('wide-gamut canvas exports preserve the native color path',()=>{
 const context=vm.createContext({});
 vm.runInContext(`class HTMLCanvasElement {constructor(){this.width=1;this.height=1;}getContext(){return {getContextAttributes(){return {colorSpace:'display-p3',colorType:'float16'};}};}toDataURL(){return this===globalThis.canvas?'native-wide-color':'converted';}}globalThis.HTMLCanvasElement=HTMLCanvasElement;(${source})({version:1,seed:'${seedA}',enabled:true,canvasNoise:true});globalThis.canvas=new HTMLCanvasElement();canvas.getContext('2d');`,context);
 assert.equal(vm.runInContext('canvas.toDataURL()',context),'native-wide-color');
});
test('webgl skips padded packs and unsupported float reads',()=>{
 const a=realm(seedA);
 const packed=Array.from(a.run('(()=>{const gl=new WebGLRenderingContext();gl.alignment=8;const out=new Uint8Array(8).fill(9);gl.readPixels(0,0,1,1,6408,5121,out);return out;})()'));
 assert.deepEqual(packed,[64,64,64,255,9,9,9,9]);
 assert.deepEqual(Array.from(a.run('(()=>{const out=new Float32Array(4).fill(.5);new WebGLRenderingContext().readPixels(0,0,1,1,6408,5126,out);return out;})()')),[.5,.5,.5,.5]);
});
test('media selection handles arrays, inherited members and call-time snapshots',async()=>{
 const devices=await realm(seedA).run('new MediaDevices().enumerateDevices()');
 const saved=devices[0].deviceId;
 const a=realm(seedA);
 const array=await a.run(`new MediaDevices().getUserMedia({video:{deviceId:[${JSON.stringify(saved)}]}})`);
 assert.deepEqual(Array.from(array.video.deviceId),['camera-real']);
 const inherited=await realm(seedA).run(`new MediaDevices().getUserMedia({video:Object.create({width:640,deviceId:{exact:${JSON.stringify(saved)}}})})`);
 assert.equal(inherited.video.width,640);assert.equal(inherited.video.deviceId.exact,'camera-real');
 const snapshot=await realm(seedA).run(`(()=>{const options={video:{width:640,deviceId:{exact:${JSON.stringify(saved)}}}};const result=new MediaDevices().getUserMedia(options);options.video.width=1280;options.video.deviceId.exact='changed';return result;})()`);
 assert.equal(snapshot.video.width,640);assert.equal(snapshot.video.deviceId.exact,'camera-real');
});
